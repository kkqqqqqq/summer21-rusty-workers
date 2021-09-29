

use std::collections::btree_map::Entry as BEntry;
use std::collections::hash_map::Entry as HEntry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::errors::{Error, Result};
use crate::metrics::Collector;
use crate::proto;

use cfg_if::cfg_if;
use lazy_static::lazy_static;

struct RegistryCore {
    pub collectors_by_id: HashMap<u64, Box<dyn Collector>>,
    pub dim_hashes_by_name: HashMap<String, u64>,
    pub desc_ids: HashSet<u64>,
    /// Optional common labels for all registered collectors.
    pub labels: Option<HashMap<String, String>>,
    /// Optional common namespace for all registered collectors.
    pub prefix: Option<String>,
}

impl std::fmt::Debug for RegistryCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RegistryCore ({} collectors)",
            self.collectors_by_id.keys().len()
        )
    }
}

impl RegistryCore {

   fn contains(&mut self, c: Box<dyn Collector>)->bool{

    for desc in c.desc() {
        // Is the desc_id unique?
        // (In other words: Is the fqName + constLabel combination unique?)
        if self.desc_ids.contains(&desc.id) {
            return  true;
        }
   }
   false
}

    fn register(&mut self, c: Box<dyn Collector>) -> Result<()> {
        let mut desc_id_set = HashSet::new();
        let mut collector_id: u64 = 0;

        
        for desc in c.desc() {
            // Is the desc_id unique?
            // (In other words: Is the fqName + constLabel combination unique?)
            if self.desc_ids.contains(&desc.id) {
                return Err(Error::AlreadyReg);
            }

            if let Some(hash) = self.dim_hashes_by_name.get(&desc.fq_name) {
                if *hash != desc.dim_hash {
                    return Err(Error::Msg(format!(
                        "a previously registered descriptor with the \
                         same fully-qualified name as {:?} has \
                         different label names or a different help \
                         string",
                        desc
                    )));
                }
            }

            self.dim_hashes_by_name
                .insert(desc.fq_name.clone(), desc.dim_hash);

            // If it is not a duplicate desc in this collector, add it to
            // the collector_id.
            if desc_id_set.insert(desc.id) {
                // The set did not have this value present, true is returned.
                collector_id = collector_id.wrapping_add(desc.id);
            } else {
                // The set did have this value present, false is returned.
                //
                // TODO: Should we allow duplicate descs within the same collector?
                return Err(Error::Msg(format!(
                    "a duplicate descriptor within the same \
                     collector the same fully-qualified name: {:?}",
                    desc.fq_name
                )));
            }
        }

        match self.collectors_by_id.entry(collector_id) {
            HEntry::Vacant(vc) => {
                self.desc_ids.extend(desc_id_set);
                vc.insert(c);
                Ok(())
            }
            HEntry::Occupied(_) => Err(Error::AlreadyReg),
        }
    }

    fn unregister(&mut self, c: Box<dyn Collector>) -> Result<()> {
        let mut id_set = Vec::new();
        let mut collector_id: u64 = 0;
        for desc in c.desc() {
            if !id_set.iter().any(|id| *id == desc.id) {
                id_set.push(desc.id);
                collector_id = collector_id.wrapping_add(desc.id);
            }
        }

        if self.collectors_by_id.remove(&collector_id).is_none() {
            return Err(Error::Msg(format!(
                "collector {:?} is not registered",
                c.desc()
            )));
        }

        for id in id_set {
            self.desc_ids.remove(&id);
        }

        // dim_hashes_by_name is left untouched as those must be consistent
        // throughout the lifetime of a program.
        Ok(())
    }

    fn gather(&self) -> Vec<proto::MetricFamily> {
        let mut mf_by_name = BTreeMap::new();

        for c in self.collectors_by_id.values() {
            let mfs = c.collect();
            for mut mf in mfs {
                // Prune empty MetricFamilies.
                if mf.get_metric().is_empty() {
                    continue;
                }

                let name = mf.get_name().to_owned();
                match mf_by_name.entry(name) {
                    BEntry::Vacant(entry) => {
                        entry.insert(mf);
                    }
                    BEntry::Occupied(mut entry) => {
                        let existent_mf = entry.get_mut();
                        let existent_metrics = existent_mf.mut_metric();

                        // TODO: check type.
                        // TODO: check consistency.
                        for metric in mf.take_metric().into_iter() {
                            existent_metrics.push(metric);
                        }
                    }
                }
            }
        }

        // TODO: metric_family injection hook.

        // Now that MetricFamilies are all set, sort their Metrics
        // lexicographically by their label values.
        for mf in mf_by_name.values_mut() {
            mf.mut_metric().sort_by(|m1, m2| {
                let lps1 = m1.get_label();
                let lps2 = m2.get_label();

                if lps1.len() != lps2.len() {
                    // This should not happen. The metrics are
                    // inconsistent. However, we have to deal with the fact, as
                    // people might use custom collectors or metric family injection
                    // to create inconsistent metrics. So let's simply compare the
                    // number of labels in this case. That will still yield
                    // reproducible sorting.
                    return lps1.len().cmp(&lps2.len());
                }

                for (lp1, lp2) in lps1.iter().zip(lps2.iter()) {
                    if lp1.get_value() != lp2.get_value() {
                        return lp1.get_value().cmp(lp2.get_value());
                    }
                }

                // We should never arrive here. Multiple metrics with the same
                // label set in the same scrape will lead to undefined ingestion
                // behavior. However, as above, we have to provide stable sorting
                // here, even for inconsistent metrics. So sort equal metrics
                // by their timestamp, with missing timestamps (implying "now")
                // coming last.
                m1.get_timestamp_ms().cmp(&m2.get_timestamp_ms())
            });
        }

        // Write out MetricFamilies sorted by their name.
        mf_by_name
            .into_iter()
            .map(|(_, mut m)| {
                // Add registry namespace prefix, if any.
                if let Some(ref namespace) = self.prefix {
                    let prefixed = format!("{}_{}", namespace, m.get_name());
                    m.set_name(prefixed);
                }

                // Add registry common labels, if any.
                if let Some(ref hmap) = self.labels {
                    let pairs: Vec<proto::LabelPair> = hmap
                        .iter()
                        .map(|(k, v)| {
                            let mut label = proto::LabelPair::default();
                            label.set_name(k.to_string());
                            label.set_value(v.to_string());
                            label
                        })
                        .collect();

                    for metric in m.mut_metric().iter_mut() {
                        let mut labels: Vec<_> = metric.take_label().into();
                        labels.append(&mut pairs.clone());
                        metric.set_label(labels.into());
                    }
                }
                m
            })
            .collect()
    }

    
}

/// A struct for registering Prometheus collectors, collecting their metrics, and gathering
/// them into `MetricFamilies` for exposition.
#[derive(Clone, Debug)]
pub struct Registry {
    r: Arc<RwLock<RegistryCore>>,
}

impl Default for Registry {
    fn default() -> Registry {
        let r = RegistryCore {
            collectors_by_id: HashMap::new(),
            dim_hashes_by_name: HashMap::new(),
            desc_ids: HashSet::new(),
            labels: None,
            prefix: None,
        };

        Registry {
            r: Arc::new(RwLock::new(r)),
        }
    }
}

impl Registry {
    /// `new` creates a Registry.
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Create a new registry, with optional custom prefix and labels.
    pub fn new_custom(
        prefix: Option<String>,
        labels: Option<HashMap<String, String>>,
    ) -> Result<Registry> {
        if let Some(ref namespace) = prefix {
            if namespace.is_empty() {
                return Err(Error::Msg("empty prefix namespace".to_string()));
            }
        }

        let reg = Registry::default();
        {
            let mut core = reg.r.write();
            core.prefix = prefix;
            core.labels = labels;
        }
        Ok(reg)
    }

    /// `register` registers a new [`Collector`] to be included in metrics
    /// collection. It returns an error if the descriptors provided by the
    /// [`Collector`] are invalid or if they — in combination with descriptors of
    /// already registered Collectors — do not fulfill the consistency and
    /// uniqueness criteria described in the documentation of [`Desc`](crate::core::Desc).
    ///
    /// If the provided [`Collector`] is equal to a [`Collector`] already registered
    /// (which includes the case of re-registering the same [`Collector`]), the
    /// AlreadyReg error returns.
    pub fn register(&self, c: Box<dyn Collector>) -> Result<()> {
        self.r.write().register(c)
    }

    /// `unregister` unregisters the [`Collector`] that equals the [`Collector`] passed
    /// in as an argument.  (Two Collectors are considered equal if their
    /// Describe method yields the same set of descriptors.) The function
    /// returns error when the [`Collector`] is not registered.
    pub fn unregister(&self, c: Box<dyn Collector>) -> Result<()> {
        self.r.write().unregister(c)
    }

    /// `gather` calls the Collect method of the registered Collectors and then
    /// gathers the collected metrics into a lexicographically sorted slice
    /// of MetricFamily protobufs.
    pub fn gather(&self) -> Vec<proto::MetricFamily> {
        self.r.read().gather()
    }

    /// `contains` is used for checking wheather the registry contains the metric
    pub fn contains(&self, c: Box<dyn Collector>)->bool{
        self.r.write().contains(c)
    }
}

cfg_if! {
    if #[cfg(all(feature = "process", target_os="linux"))] {
        fn register_default_process_collector(reg: &Registry) -> Result<()> {
            use crate::process_collector::ProcessCollector;

            let pc = ProcessCollector::for_self();
            reg.register(Box::new(pc))
        }
    } else {
        fn register_default_process_collector(_: &Registry) -> Result<()> {
            Ok(())
        }
    }
}

// Default registry for rust-prometheus.
lazy_static! {
    static ref DEFAULT_REGISTRY: Registry = {
        let reg = Registry::default();

        // Register a default process collector.
        register_default_process_collector(&reg).unwrap();

        reg
    };
}

/// Default registry (global static).
pub fn default_registry() -> &'static Registry {
    lazy_static::initialize(&DEFAULT_REGISTRY);
    &DEFAULT_REGISTRY
}

/// Registers a new [`Collector`] to be included in metrics collection. It
/// returns an error if the descriptors provided by the [`Collector`] are invalid or
/// if they - in combination with descriptors of already registered Collectors -
/// do not fulfill the consistency and uniqueness criteria described in the
/// [`Desc`](crate::core::Desc) documentation.
pub fn register(c: Box<dyn Collector>) -> Result<()> {
    DEFAULT_REGISTRY.register(c)
}

/// Unregisters the [`Collector`] that equals the [`Collector`] passed in as
/// an argument. (Two Collectors are considered equal if their Describe method
/// yields the same set of descriptors.) The function returns an error if a
/// [`Collector`] was not registered.
pub fn unregister(c: Box<dyn Collector>) -> Result<()> {
    DEFAULT_REGISTRY.unregister(c)
}

/// Return all `MetricFamily` of `DEFAULT_REGISTRY`.
pub fn gather() -> Vec<proto::MetricFamily> {
    DEFAULT_REGISTRY.gather()
}
pub fn contains( c: Box<dyn Collector>)->bool{
    DEFAULT_REGISTRY.contains(c)
  }




#[macro_use]
extern crate log;

mod config;
mod sched;

use anyhow::Result;
use once_cell::sync::OnceCell;
use rusty_workers::types::*;
use std::net::SocketAddr;
use std::sync::Arc;
use structopt::StructOpt;
use tokio::net::lookup_host;

use std::time::Instant;
use crate::config::*;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server,Request};
use sched::SchedError;

//about promrtheus
use std::collections::HashMap;
use prometheus::{Encoder, Registry,IntGauge,TextEncoder};
use lazy_static::lazy_static;



pub static SCHEDULER: OnceCell<Arc<sched::Scheduler>> = OnceCell::new();

#[derive(Debug, StructOpt)]
#[structopt(name = "rusty-workers-proxy", about = "Rusty Workers (frontend proxy)")]
struct Opt {
    /// HTTP listen address.
    #[structopt(short = "l", long)]
    http_listen: SocketAddr,

    #[structopt(long, env = "RW_FETCH_SERVICE")]
    fetch_service: String,

    /// Runtime service backends, comma-separated.
    #[structopt(long, env = "RUNTIMES")]
    runtimes: String,

    /// Max ArrayBuffer memory per worker, in MB
    #[structopt(long, env = "RW_MAX_AB_MEMORY_MB", default_value = "16")]
    max_ab_memory_mb: u32,

    /// Max CPU time, in milliseconds
    #[structopt(long, env = "RW_MAX_TIME_MS", default_value = "100")]
    max_time_ms: u32,

    /// Max number of concurrent I/O operations
    #[structopt(long, env = "RW_MAX_IO_CONCURRENCY", default_value = "10")]
    max_io_concurrency: u32,

    /// Max number of I/O operations per request
    #[structopt(long, env = "RW_MAX_IO_PER_REQUEST", default_value = "50")]
    max_io_per_request: u32,

    /// Max ready instances per app
    #[structopt(long, env = "RW_MAX_READY_INSTANCES_PER_APP", default_value = "50")]
    max_ready_instances_per_app: usize,

    /// Expiration time for ready instances
    #[structopt(
        long,
        env = "RW_READY_INSTANCE_EXPIRATION_MS",
        default_value = "120000"
    )]
    ready_instance_expiration_ms: u64,

    /// Request timeout in milliseconds.
    #[structopt(long, env = "RW_REQUEST_TIMEOUT_MS", default_value = "30000")]
    pub request_timeout_ms: u64,

    /// Max request body size in bytes.
    #[structopt(
        long,
        env = "RW_MAX_REQUEST_BODY_SIZE_BYTES",
        default_value = "2097152"
    )]
    pub max_request_body_size_bytes: u64,

    /// Probability of an instance being dropped out after a request. Valid values are 0 to 1.
    #[structopt(long, env = "RW_DROPOUT_RATE", default_value = "0.001")]
    pub dropout_rate: f32,

    /// Routing cache size.
    #[structopt(long, env = "RW_ROUTE_CACHE_SIZE", default_value = "1000")]
    pub route_cache_size: usize,

    /// MySQL-compatible database URL.
    #[structopt(long, env = "RW_DB_URL")]
    pub db_url: String,

    /// Size of app cache.
    #[structopt(long, env = "RW_APP_CACHE_SIZE", default_value = "100")]
    pub app_cache_size: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init_timed();
    rusty_workers::init();
    let opt = Opt::from_args();
    /**************************/
    let prometheus_addr = ([127, 0, 0, 1], 9898).into();
    info!("prometheus metrics output :http://{}", prometheus_addr);
     
    /**************************/
    let mut runtime_cluster: Vec<SocketAddr> = Vec::new();
    for elem in opt.runtimes.split(",") {
        let runtime_addr = lookup_host(elem)
            .await?
            .next()
            .unwrap_or_else(|| panic!("runtime address lookup failed: {}", elem));
        runtime_cluster.push(runtime_addr);
    }

    let kv_client = rusty_workers::db::DataClient::new(&opt.db_url).await?;

    let fetch_service = lookup_host(&opt.fetch_service)
        .await?
        .next()
        .expect("fetch service unresolved");

    SCHEDULER
        .set(sched::Scheduler::new(
            WorkerConfiguration {
                executor: ExecutorConfiguration {
                    max_ab_memory_mb: opt.max_ab_memory_mb,
                    max_time_ms: opt.max_time_ms,
                    max_io_concurrency: opt.max_io_concurrency,
                    max_io_per_request: opt.max_io_per_request,
                },
                fetch_service,
                env: Default::default(),
                kv_namespaces: Default::default(),
            },
            LocalConfig {
                max_ready_instances_per_app: opt.max_ready_instances_per_app,
                ready_instance_expiration_ms: opt.ready_instance_expiration_ms,
                request_timeout_ms: opt.request_timeout_ms,
                max_request_body_size_bytes: opt.max_request_body_size_bytes,
                dropout_rate: opt.dropout_rate,
                route_cache_size: opt.route_cache_size,
                app_cache_size: opt.app_cache_size,
                runtime_cluster,
            },
            kv_client,
        ))
        .unwrap_or_else(|_| panic!("cannot set scheduler"));

    tokio::spawn(async move {
        loop {
            let scheduler = SCHEDULER.get().unwrap();
            scheduler.discover_runtimes().await;
            scheduler.query_runtimes().await;
            tokio::time::sleep(std::time::Duration::from_secs(7)).await;
        }
    });

    //prometheus
    tokio::spawn(async move {
        loop {
            lazy_static! {
                static ref APP_NUM: IntGauge = IntGauge::new("APP_NUM", "the number of apps running on rusty-workers").unwrap();
            }
            //registry
            if(! prometheus::default_registry().contains(Box::new(APP_NUM.clone()))){
                prometheus::default_registry().register(Box::new(APP_NUM.clone())).unwrap(); 
            }
             

            let scheduler = SCHEDULER.get().unwrap();

            APP_NUM.set(scheduler.apps.lock().await.len() as i64);
     
            for (app,appstate) in scheduler.apps.lock().await.iter()  {

                lazy_static! {
                    static ref APP_LAST_TIME: IntGauge = IntGauge::new("APP_LAST_TIME", "the running time  of apps running on rusty-workers").unwrap();
                    static ref READY_INSTANCE : IntGauge =  IntGauge::new("READY_INSTANCE", "the usage of memory of rusty-workers").unwrap();
                }

                //registry
                if(! prometheus::default_registry().contains(Box::new(APP_LAST_TIME.clone()))){
                    prometheus::default_registry().register(Box::new(APP_LAST_TIME.clone())).unwrap(); 
                }
                if(! prometheus::default_registry().contains(Box::new(READY_INSTANCE.clone()))){
                    prometheus::default_registry().register(Box::new(READY_INSTANCE.clone())).unwrap();       
                }
               
                APP_LAST_TIME.set((Instant::now()-appstate.start_time).as_secs() as i64);
                READY_INSTANCE.set(appstate.ready_instances.lock().await.len() as i64);
                    
            
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
        
    );

    tokio::spawn(async move {
        
        /*************** prometheus ***************/
        let serve_future = Server::bind(&prometheus_addr).serve(make_service_fn(|_| async {
            Ok::<_, hyper::Error>(service_fn(prometheus_serve_req))
        }));
        if let Err(err) = serve_future.await {
            eprintln!("server error: {}", err);
        } 
        
    });


    


    let make_svc = make_service_fn(|_| async move {
        Ok::<_, hyper::Error>(service_fn(|req| async move {
            let scheduler = SCHEDULER.get().unwrap();
            match scheduler.handle_request(req).await {
                Ok(x) => Ok::<_, hyper::Error>(x),
                Err(e) => {
                    debug!("handle_request failed: {:?}", e);
                    let res = match e.downcast::<SchedError>() {
                        Ok(e) => e.build_response(),
                        Err(_) => {
                            let mut res = Response::new(Body::from("internal server error"));
                            *res.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                            res
                        }
                    };
                    Ok::<_, hyper::Error>(res)
                }
            }
        }))
    });
    info!("starting http server");

    Server::bind(&opt.http_listen).serve(make_svc).await?;
    Ok(())

    
}





async fn prometheus_serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let encoder = TextEncoder::new();

    //HTTP_COUNTER.inc();
    //let timer = HTTP_REQ_HISTOGRAM.with_label_values(&["all"]).start_timer();

    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    //HTTP_BODY_GAUGE.set(buffer.len() as f64);

    let response = Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();

    //timer.observe_duration();

    Ok(response)
}

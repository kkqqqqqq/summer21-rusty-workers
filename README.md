#基于 Grafana 与 Prometheus 实现 rusty-workers 运行指标的监控和可视化


[![Build and Test](https://github.com/losfair/rusty-workers/workflows/Build%20and%20Test/badge.svg)](https://github.com/losfair/rusty-workers/actions)

A cloud-native distributed serverless workers platform.


## Getting started

### Prerequisites

- [Rust](https://www.rust-lang.org/) nightly >= 1.50
- [Node.js](https://nodejs.org/) and [npm](https://www.npmjs.com/)
- A MySQL-compatible database server. [TiDB](https://github.com/pingcap/tidb) is recommended for clusters but you can also use anything else, e.g. AWS Aurora or just MySQL itself.

### Build

```bash
cd librt
npm intall  
cd ..
make librt-deps
make

```
you should chaeck the submodule
```
git submodule update --init --recursive
```

you can find the these built binaries in `target/release` :

- `rusty-workers-proxy`
- `rusty-workers-runtime`
- `rusty-workers-fetchd`
- `rusty-workers-cli`

open your database and create a database named "rusty_workers"
and alert your database by the sql file in the sql dir of the repository

### Start services

u can see the  `run_all.sh` as an example of getting everything up and running. 
when u start prometheus,please use the confug file in the  prometheus_config dir  of the repository.
```
./prometheus --config.file=prometheus.yml

```


### Deploy your first application
```
export DB_URL="mysql://root@localhost:4000/rusty_workers"

./target/release/rusty-workers-cli app add-single-file-app ./counter.toml --js ./counter.js

./target/release/rusty-workers-cli app add-route localhost --path /counter --appid 19640b0c-1dff-4b20-9599-0b4c4a11da3f

# Open a browser and navigate to http://localhost:3280/counter !
```



### result
open the browser  localhost:9090 to see the prometheus



## for more information , please turn to the report in the doc dir

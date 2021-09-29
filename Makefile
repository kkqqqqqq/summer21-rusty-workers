all: runtime cli fetchd proxy prometheus # cp playground-api

runtime:
	cd librt && npm run build
#	cd rusty-workers-runtime && V8_FROM_SOURCE=1 CLANG_BASE_PATH=/usr cargo build --release
	cd rusty-workers-runtime && cargo build --release

cli:
	cd rusty-workers-cli && cargo build --release

fetchd:
	cd rusty-workers-fetchd && cargo build --release

proxy:
	cd rusty-workers-proxy && cargo build --release

cp:
	cd rusty-workers-cp && cargo build --release

playground-api:
	cd rusty-workers-playground-api && cargo build --release

librt-deps:
	cd librt && npm install




# Split docker build from the `all` target for now since I build them on two different VMs
docker:
	./build_docker.sh

prometheus:
	cd prometheus && cargo build --release

.PHONY: runtime cli fetchd proxy cp playground-api librt-deps docker prometheus


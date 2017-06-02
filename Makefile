default: build docker.build
build:
	@cargo clean
	@docker run --rm -it -v $(shell pwd):/home/rust/src ekidd/rust-musl-builder cargo build --release
docker.build:
	@docker build -t 127labs/yakp .
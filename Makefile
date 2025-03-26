build-x86_64:
	cross build --bin eleel --target x86_64-unknown-linux-gnu  --profile release --locked

build-aarch64:
	cross build --bin eleel --target aarch64-unknown-linux-gnu  --profile release --locked

build: build-x86_64 build-aarch64

.PHONY: build-x86_64 build-aarch64

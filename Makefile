.PHONY: fmt check test media-validation benchmark ui-build

fmt:
	cargo fmt --all -- --check

check:
	cargo check --workspace

test:
	cargo test --workspace

media-validation:
	bash scripts/validate-media-pipeline.sh

benchmark:
	bash scripts/benchmark-core.sh

ui-build:
	npm --prefix apps/desktop run build

.PHONY: check release

check:
	cargo fmt --check
	cargo clippy
	cargo test

release:
ifndef version
	$(error version is not set. Usage: make release version=<version>)
endif
	cargo set-version $(version)
	git tag $(version)

.PHONY: check release

check:
	cargo fmt --check
	cargo clippy
	cargo test

release:
ifndef version
	$(error version is not set. Usage: make release version=<version> msg="<msg>")
endif
ifndef msg
	$(error msg is not set. Usage: make release version=<version> msg="<msg>")
endif
	@full_version=$(shell echo $(version) | grep -q '\.' && echo "0.$(version)" || echo "0.$(version).0"); \
	echo $$full_version; \
	cargo set-version $$full_version; \
	git commit -a -m "Bump version to $$full_version" ; \
	git tag $(version) -m "$(msg)"
	echo "Run 'git push' and 'git push --tag' afterwards."

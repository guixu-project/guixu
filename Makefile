.PHONY: build test package clean

# Build the project
build:
	cargo build --release

# Run tests
test:
	cargo test

# Package plugins for Codex Desktop and Claude Cowork
package:
	./scripts/package-plugins.sh --output dist --clean

# Package only Codex plugin
package-codex:
	./scripts/package-plugins.sh --output dist --type codex --clean

# Package only Claude plugin
package-claude:
	./scripts/package-plugins.sh --output dist --type claude --clean

# Clean build artifacts and packaged plugins
clean:
	cargo clean
	rm -rf dist

# Install the binary
install:
	cargo install --path crates/node

# Install plugins
install-plugins:
	./scripts/install-plugins.sh --both

# Run linter
lint:
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Check formatting
fmt-check:
	cargo fmt --check

# Run all checks
check: fmt-check lint test

# Help
help:
	@echo "Available targets:"
	@echo "  build          - Build the project"
	@echo "  test           - Run tests"
	@echo "  package        - Package plugins for Codex and Claude"
	@echo "  package-codex  - Package only Codex plugin"
	@echo "  package-claude - Package only Claude plugin"
	@echo "  clean          - Clean build artifacts and packaged plugins"
	@echo "  install        - Install the binary"
	@echo "  install-plugins - Install plugins for Codex and Claude"
	@echo "  lint           - Run linter"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check formatting"
	@echo "  check          - Run all checks (fmt, lint, test)"
	@echo "  help           - Show this help message"
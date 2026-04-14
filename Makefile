# ByeDroid TUI — install and dev tasks
# Requires Rust toolchain (cargo) on PATH.

PREFIX ?= /usr/local
USER_BIN ?= $(HOME)/.local/bin
REL_VERSION ?= 0.1.0

.PHONY: default install install-user install-system uninstall uninstall-user uninstall-system build test clippy clean fmt check release

default: build

# Install via Cargo into ~/.cargo/bin (same as `cargo install --path .`).
# Ensure ~/.cargo/bin is on your PATH (rustup usually adds this).
install:
	cargo install --path . --force

uninstall:
	cargo uninstall byedroid 2>/dev/null || true

# Copy the release binary to ~/.local/bin — no sudo. Add ~/.local/bin to PATH if needed.
install-user: build
	mkdir -p $(USER_BIN)
	install -m 755 target/release/bd $(USER_BIN)/bd
	@echo "Installed $(USER_BIN)/bd"
	@echo "If \`bd\` is not found, add this to your shell config: export PATH=\"$(USER_BIN):\$$PATH\""

uninstall-user:
	rm -f $(USER_BIN)/bd

# System-wide install (default: /usr/local/bin). Requires sudo on macOS/Linux.
install-system: build
	sudo install -m 755 target/release/bd $(PREFIX)/bin/bd
	@echo "Installed $(PREFIX)/bin/bd"

uninstall-system:
	sudo rm -f $(PREFIX)/bin/bd

build:
	cargo build --release

test:
	cargo nextest run

clippy:
	cargo clippy --all-targets

fmt:
	cargo fmt

# fmt + clippy + test
check: fmt clippy test

clean:
	cargo clean

# Build release tarballs for Homebrew (host + cross-targets when installed).
# Update sha256 placeholders in `homebrew-tap/Formula/byedroid.rb` from the output.
release: build
	mkdir -p dist
	HOST=$$(rustc -vV | sed -n 's/^host: //p'); \
	tar -czvf "dist/byedroid-$(REL_VERSION)-$$HOST.tar.gz" -C target/release bd && \
	shasum -a 256 "dist/byedroid-$(REL_VERSION)-$$HOST.tar.gz"
	@for triple in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu; do \
		if rustup target list --installed 2>/dev/null | grep -q "^$$triple$$"; then \
			echo "Building $$triple..."; \
			cargo build --release --target "$$triple" && \
			tar -czvf "dist/byedroid-$(REL_VERSION)-$$triple.tar.gz" -C "target/$$triple/release" bd && \
			shasum -a 256 "dist/byedroid-$(REL_VERSION)-$$triple.tar.gz"; \
		fi; \
	done

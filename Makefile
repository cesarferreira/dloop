# Droid Loop TUI — install and dev tasks
# Requires Rust toolchain (cargo) on PATH.

PREFIX ?= /usr/local
USER_BIN ?= $(HOME)/.local/bin

.PHONY: default install install-user install-system uninstall uninstall-user uninstall-system build test clippy clean fmt check

default: build

# Install via Cargo into ~/.cargo/bin (same as `cargo install --path .`).
# Ensure ~/.cargo/bin is on your PATH (rustup usually adds this).
install:
	cargo install --path . --force

uninstall:
	cargo uninstall loopcat 2>/dev/null || true

# Copy the release binary to ~/.local/bin — no sudo. Add ~/.local/bin to PATH if needed.
install-user: build
	mkdir -p $(USER_BIN)
	install -m 755 target/release/dloop $(USER_BIN)/dloop
	@echo "Installed $(USER_BIN)/dloop"
	@echo "If \`dloop\` is not found, add this to your shell config: export PATH=\"$(USER_BIN):\$$PATH\""

uninstall-user:
	rm -f $(USER_BIN)/dloop

# System-wide install (default: /usr/local/bin). Requires sudo on macOS/Linux.
install-system: build
	sudo install -m 755 target/release/dloop $(PREFIX)/bin/dloop
	@echo "Installed $(PREFIX)/bin/dloop"

uninstall-system:
	sudo rm -f $(PREFIX)/bin/dloop

build:
	cargo build --release

test:
	cargo test

clippy:
	cargo clippy --all-targets

fmt:
	cargo fmt

# fmt + clippy + test
check: fmt clippy test

clean:
	cargo clean

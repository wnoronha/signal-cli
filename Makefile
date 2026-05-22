# Makefile for signal-cli (Rust)

PREFIX ?= $(HOME)/.local
BINDIR = $(PREFIX)/bin
MANDIR1 = $(PREFIX)/share/man/man1
MANDIR5 = $(PREFIX)/share/man/man5
PKG_NAME = signal-cli

.PHONY: all build test clean install uninstall

all: build

build:
	cargo build --release

test:
	cargo test

clean:
	cargo clean

install: build
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 target/release/$(PKG_NAME) $(DESTDIR)$(BINDIR)/$(PKG_NAME)
	@if command -v asciidoctor >/dev/null 2>&1; then \
		install -d $(DESTDIR)$(MANDIR1); \
		install -d $(DESTDIR)$(MANDIR5); \
		asciidoctor -b manpage docs/signal-cli.1.adoc -o docs/signal-cli.1; \
		asciidoctor -b manpage docs/signal-cli-jsonrpc.5.adoc -o docs/signal-cli-jsonrpc.5; \
		install -m 644 docs/signal-cli.1 $(DESTDIR)$(MANDIR1)/signal-cli.1; \
		install -m 644 docs/signal-cli-jsonrpc.5 $(DESTDIR)$(MANDIR5)/signal-cli-jsonrpc.5; \
		echo "Installed binary and man pages."; \
	else \
		echo "Installed binary. (asciidoctor not found, skipping man page generation)"; \
	fi

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(PKG_NAME)
	rm -f $(DESTDIR)$(MANDIR1)/signal-cli.1
	rm -f $(DESTDIR)$(MANDIR5)/signal-cli-jsonrpc.5

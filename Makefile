DAEMON_BINARY := swhkd
SERVER_BINARY := swhks
BUILDFLAGS := --release
POLKIT_DIR := /usr/share/polkit-1/actions
POLKIT_POLICY_FILE := com.github.swhkd.pkexec.policy
TARGET_DIR := /usr/bin
# Remember to edit this ^ in policy file too.

all: build

build:
	@cargo build $(BUILDFLAGS) --target=x86_64-unknown-linux-musl
	@cp ./target/x86_64-unknown-linux-musl/release/$(DAEMON_BINARY) ./bin/$(DAEMON_BINARY)
	@cp ./target/x86_64-unknown-linux-musl/release/$(SERVER_BINARY) ./bin/$(SERVER_BINARY)

glibc:
	@cargo build $(BUILDFLAGS)
	@cp ./target/release/$(DAEMON_BINARY) ./bin/$(DAEMON_BINARY)
	@cp ./target/release/$(SERVER_BINARY) ./bin/$(SERVER_BINARY)

install:
	@mkdir -p $(TARGET_DIR)
	@mkdir -p $(POLKIT_DIR)
	@mkdir -p /etc/$(DAEMON_BINARY)
	@touch /etc/$(DAEMON_BINARY)/$(DAEMON_BINARY)rc
	@cp ./bin/$(DAEMON_BINARY) $(TARGET_DIR)
	@cp ./bin/$(SERVER_BINARY) $(TARGET_DIR)
	@cp ./$(POLKIT_POLICY_FILE) $(POLKIT_DIR)/$(POLKIT_POLICY_FILE)
	@chmod +x $(TARGET_DIR)/$(DAEMON_BINARY)
	@chmod +x $(TARGET_DIR)/$(SERVER_BINARY)

uninstall:
	@rm $(TARGET_DIR)/$(SERVER_BINARY)
	@rm $(TARGET_DIR)/$(DAEMON_BINARY)
	@rm $(POLKIT_DIR)/$(POLKIT_POLICY_FILE)

check:
	@cargo fmt
	@cargo check --target=x86_64-unknown-linux-musl
	@cargo clippy

clean:
	@cargo clean

setup:
	@mkdir -p ./bin
	@rustup install stable
	@rustup default stable
	@rustup target add x86_64-unknown-linux-musl

.PHONY: check clean setup all install build glibc

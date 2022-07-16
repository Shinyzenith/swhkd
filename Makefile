DAEMON_BINARY := swhkd
SERVER_BINARY := swhks
BUILDFLAGS := --release
POLKIT_DIR := /usr/share/polkit-1/actions
POLKIT_POLICY_FILE := com.github.swhkd.pkexec.policy
# Remember to edit the TARGET_DIR in policy file too if you do change it.
TARGET_DIR := /usr/bin
MAN1_DIR := /usr/share/man/man1
MAN5_DIR := /usr/share/man/man5
VERSION = $(shell awk -F ' = ' '$$1 ~ /version/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml)

all: build

build:
	@cargo build $(BUILDFLAGS)

install:
	@scdoc < ./docs/$(DAEMON_BINARY).1.scd > ./docs/$(DAEMON_BINARY).1.gz
	@scdoc < ./docs/$(SERVER_BINARY).1.scd > ./docs/$(SERVER_BINARY).1.gz
	@scdoc < ./docs/$(DAEMON_BINARY).5.scd > ./docs/$(DAEMON_BINARY).5.gz
	@scdoc < ./docs/$(DAEMON_BINARY)-keys.5.scd > ./docs/$(DAEMON_BINARY)-keys.5.gz
	@mkdir -p $(MAN1_DIR)
	@mkdir -p $(MAN5_DIR)
	@mkdir -p $(POLKIT_DIR)
	@mkdir -p $(TARGET_DIR)
	@mkdir -p /etc/$(DAEMON_BINARY)
	@mv ./docs/$(DAEMON_BINARY).1.gz $(MAN1_DIR)
	@mv ./docs/$(SERVER_BINARY).1.gz $(MAN1_DIR)
	@mv ./docs/$(DAEMON_BINARY).5.gz $(MAN5_DIR)
	@mv ./docs/$(DAEMON_BINARY)-keys.5.gz $(MAN5_DIR)
	@touch /etc/$(DAEMON_BINARY)/$(DAEMON_BINARY)rc
	@cp ./target/release/$(DAEMON_BINARY) $(TARGET_DIR)
	@cp ./target/release/$(SERVER_BINARY) $(TARGET_DIR)
	@cp ./$(POLKIT_POLICY_FILE) $(POLKIT_DIR)/$(POLKIT_POLICY_FILE)
	@chmod +x $(TARGET_DIR)/$(DAEMON_BINARY)
	@chmod +x $(TARGET_DIR)/$(SERVER_BINARY)

uninstall:
	@rm -f $(MAN1_DIR)/$(DAEMON_BINARY).1.gz
	@rm -f $(MAN1_DIR)/$(SERVER_BINARY).1.gz
	@rm -f $(MAN5_DIR)/$(DAEMON_BINARY).5.gz
	@rm -f $(MAN5_DIR)/$(DAEMON_BINARY)-keys.5.gz
	@rm $(TARGET_DIR)/$(SERVER_BINARY)
	@rm $(TARGET_DIR)/$(DAEMON_BINARY)
	@rm $(POLKIT_DIR)/$(POLKIT_POLICY_FILE)

check:
	@cargo fmt
	@cargo check
	@cargo clippy

release:
	@rm -f Cargo.lock
	@$(MAKE) -s
	@zip -r "glibc-x86_64-$(VERSION).zip" ./target/release/swhkd ./target/release/swhks

test:
	@cargo test

clean:
	@cargo clean

setup:
	@rustup install stable
	@rustup default stable

.PHONY: check clean setup all install build release

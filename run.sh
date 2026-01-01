#!/usr/bin/env bash

set -euxo pipefail

RUST_LOG_STYLE=always RUST_LOG=info ./target/debug/xecut_bot |& tee -ia xecut_bot.log

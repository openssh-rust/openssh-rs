#!/bin/bash -ex

cd "$(dirname "$(realpath "$0")")"

cargo check --tests --all-features
cargo clippy --all-features
exec cargo test --all-features

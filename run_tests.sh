#!/bin/bash

export HOSTNAME=openssh
export TEST_HOST=ssh://test-user@`dig +short $HOSTNAME`:2222

cd $(dirname `realpath $0`)

sleep 1

echo Test ssh connection
chmod 600 .test-key
ssh -i .test-key -v -p 2222 -l test-user $HOSTNAME \
    -o StrictHostKeyChecking=accept-new whoami

echo Set up ssh agent
eval $(ssh-agent)
cat .test-key | ssh-add -

echo Run tests
for each in mux_client_impl process_impl; do
    rm -rf $each/control-test $each/config-file-test $each/.ssh-connection*
done

mkdir -p ci-cargo-home

export RUSTFLAGS='--cfg=ci'
export CARGO_HOME="$(realpath ci-cargo-home)"

echo Running openssh-process
cargo test \
    --target-dir ./ci-target \
    --no-fail-fast \
    --test openssh-process \
    -- --test-threads=3 # Use test-threads=3 so that the output is readable

echo Running openssh-mux-client
exec cargo test \
    --all-features \
    --target-dir ./ci-target \
    --no-fail-fast \
    --test openssh-mux-client \
    -- --test-threads=3 # Use test-threads=3 so that the output is readable

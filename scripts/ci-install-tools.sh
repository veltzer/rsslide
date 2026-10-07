#!/bin/bash
# Install the system libraries this repo's build links against, the
# external tools its tests shell out to, and the rsconstruct that runs the
# build. The canonical ci.yml runs this in every repo before `rsconstruct
# build`. Keep it strict: anything that fails to install must fail the build
# here, not surface later as a confusing build or test failure.
set -euo pipefail

# The deck processors in rsconstruct.toml run `rsslide`, so the binary this
# checkout builds goes on PATH before rsconstruct looks for it: `tool
# install` below has no recipe for a tool outside its registry and would
# fail on a missing one. cargo's bin dir is the same place the rsconstruct
# binary lands, and the Build step's cargo creator reuses these artifacts.
cargo build
cp target/debug/rsslide "${CARGO_HOME:-${HOME}/.cargo}/bin/rsslide"

# ci.yml's Build step is `rsconstruct build`, so rsconstruct is the one tool
# every repo installs here. The release binary is downloaded rather than
# built (a 20 MB fetch against minutes of compile) into cargo's bin dir,
# which is on PATH and which rust-cache carries between runs together with
# everything the two install commands put there. `tool install-deps`
# installs the cargo subcommands rsconstruct.toml declares under
# [dependencies] cargo (cargo deny, cargo nextest); `tool install` installs
# the external tools of its enabled processors (rumdl, ...). Nothing here or
# in ci.yml names a tool: rsconstruct.toml is the fleet's one list of them.
#
# Test job only. TARGET is set by ci.yml's build job, which runs nothing but
# `cargo build --release`: there the crates would be compiled for no caller,
# once per release target, on a per-target cache that never holds them -
# an hour per Linux release job when this did run there (run 36876289729).
if [[ -z "${TARGET:-}" ]]; then
	bin="${CARGO_HOME:-${HOME}/.cargo}/bin"
	curl -fsSL https://github.com/veltzer/rsconstruct/releases/latest/download/rsconstruct-linux-x86_64 -o "${bin}/rsconstruct"
	chmod +x "${bin}/rsconstruct"
	rsconstruct tool install-deps
	rsconstruct tool install
fi

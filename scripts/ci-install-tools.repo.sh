#!/bin/bash
# This repo's own CI installs: the system libraries the crate links against
# and the external tools its tests shell out to. The fleet-shared
# scripts/ci-install-tools.sh runs this in every rs* repo, in the test job
# (TARGET unset) and in the release build job (TARGET names the release
# triple), after it has put the rsconstruct release binary on PATH and before
# it installs the tools rsconstruct.toml declares. Keep it strict: anything
# that fails to install must fail the build here.
set -euo pipefail

# The deck processors in rsconstruct.toml run `rsslide`, so the binary this
# checkout builds goes on PATH before the shared script's `tool install`
# looks for it: that command has no recipe for a tool outside its registry
# and would fail on a missing one. cargo's bin dir is the same place the
# rsconstruct binary lands, and the Build step's cargo creator reuses these
# artifacts. Test job only: the release build job never runs the decks.
if [[ -z "${TARGET:-}" ]]; then
	# The fonts rsslide's default config renders with (marp's look): Noto
	# Sans regular/italic (fonts-noto-core), its semibold faces
	# (fonts-noto-extra) and Liberation Mono (fonts-liberation).
	sudo apt-get -o Acquire::Retries=3 update
	sudo apt-get -o Acquire::Retries=3 install -y fonts-noto-core fonts-noto-extra fonts-liberation
	cargo build
	cp target/debug/rsslide "${CARGO_HOME:-${HOME}/.cargo}/bin/rsslide"
fi

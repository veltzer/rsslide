#!/bin/bash -eu

# Wrapper for rsconstruct generator: receives (input, output) and calls rsslide
INPUT="$1"
OUTPUT="$2"
cargo run -- -f pdf -o "$OUTPUT" "$INPUT"

#!/bin/sh

## Pre-commit
if ! cargo make --version 2&>/dev/null; then
    echo "============================="
    echo "=== Installing cargo-make ==="
    echo "=============================\n"
    cargo install cargo-make
fi

echo "============================"
echo "=== Copy pre-commit hook ==="
echo "============================\n"
cp .pre-commit.sh .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

## Commit-msg
if stat .git/hooks/commit-msg 2&>/dev/null; then
    echo "========================================"
    echo "=== Installing sailr commit-msg hook ==="
    echo "========================================\n"
    # sailr requires has `jq` to be installed
    curl -o- https://raw.githubusercontent.com/craicoverflow/sailr/master/scripts/install.sh | bash
fi

## Finish
echo "---------\n| Done! |\n---------"
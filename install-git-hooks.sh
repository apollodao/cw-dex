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
echo "========================================"
echo "=== Installing sailr commit-msg hook ==="
echo "========================================\n"
# sailr requires has `jq` to be installed
#!/bin/sh

script_file="https://raw.githubusercontent.com/apollodao/sailr/master/sailr.sh"

destination="${PWD}/.git/hooks"

download_status=$(curl $script_file -o "${destination}/commit-msg")
chmod u+x "${destination}/commit-msg"

echo -e "\nInstalled Sailr as commit-msg hook in $destination."
echo "For usage see https://github.com/craicoverflow/sailr#usage"

# Reinitialize git repo
git init

## Finish
echo "---------\n| Done! |\n---------"

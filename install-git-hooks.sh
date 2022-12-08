#!/bin/sh

hooks="${PWD}/.git/hooks"

## Pre-commit
if ! cargo make --version 2&>/dev/null; then
    echo "============================="
    echo "=== Installing cargo-make ==="
    echo "=============================\n"
    if ! cargo install cargo-make; then
        echo "\nCould not install cargo-make"
        exit 1
    fi
fi

echo "============================"
echo "=== Copy pre-commit hook ==="
echo "============================\n"
steps="clippy-check format-check machete-check"
printf "Do you want the pre-commit script to automatically correct errors (y/N)? "
read ans
if [ $ans = "y" ] || [ $ans = "Y" ] || [ $ans = "yes" ] || [$ans = "YES"]; then
    steps="clippy-fix format-fix machete-fix"
fi

sed -e "s/%steps%/${steps}/" .pre-commit.sh > "${hooks}/pre-commit"
chmod u+x "${hooks}/pre-commit"
echo


## Commit-msg
echo "========================================"
echo "=== Installing sailr commit-msg hook ==="
echo "========================================\n"

# sailr requires `jq` to be installed
script_file="https://raw.githubusercontent.com/apollodao/sailr/master/sailr.sh"

if curl $script_file -o "${hooks}/commit-msg"; then
    chmod u+x "${hooks}/commit-msg"
    echo "\nInstalled Sailr as commit-msg hook in $hooks."
    echo "For usage see https://github.com/apollodao/sailr#usage\n"
else
    echo "\nCould not install Sailr."
    exit 1
fi

# Reinitialize git repo
git init

## Finish
echo "\n---------\n| Done! |\n---------"

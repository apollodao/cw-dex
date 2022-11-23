#!/bin/sh

# Escape codes
RESET="\x1b[0m"
PASS="\x1b[37;42;1m"
FAIL="\x1b[37;41;1m"
UL="\x1b[39;49;4m"
IT="\x1b[39;49;3m"
TITLE="\x1b[36;49;1;3m"

if ! cargo make --version 2&>/dev/null; then
	echo "cargo-make is not installed. Exiting."
	exit 1
fi

makefile="$PWD/Makefile.toml"
steps="clippy format machete"

# TODO: save staged and non-staged files here and automatically
#		`git add` any new modified files between steps to avoid having the user
#		re-run their commit command

failures=false
for task in $steps; do
	echo "[${TITLE}PRE-COMMIT${RESET}] Running step ${IT}$task${RESET}....."
	cargo make --makefile $makefile -t $task | sed 's/^/    /'
	if [ $PIPESTATUS -eq 0 ]; then
		status="${PASS}PASS"
	else
		status="${FAIL}FAIL"
		failures=true
	fi
	align=`expr 79 - $(echo $task | wc -m)`
	printf "[${TITLE}PRE-COMMIT${RESET}] ${UL}$task${RESET}"
	echo "[$status${RESET}]" | sed -e :a -e "s/^.\{1,${align}\}$/.&/;ta"
done

if $failures; then
	# One or more steps failed so we can't commit yet
	exit 1
else
	# Everything went fine!
	exit 0
fi

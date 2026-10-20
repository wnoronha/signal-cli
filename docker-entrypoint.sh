#!/bin/sh
# Inject global flags from the environment, then run the given subcommand.
#   SIGNAL_ACCOUNT  -> `-a <number>` (omit for multi-account mode)
#   SIGNAL_CONFIG   -> `--config <path>` (default /data)
set -e

CONFIG="${SIGNAL_CONFIG:-/data}"

# Rebuild argv: global flags first, subcommand + flags last.
if [ -n "$SIGNAL_ACCOUNT" ]; then
    set -- -a "$SIGNAL_ACCOUNT" "$@"
fi

exec /usr/local/bin/signal-cli --config "$CONFIG" "$@"

#!/usr/bin/env bash
OD=/tmp/mrpc-eval
if [[ $# -ge 1 ]]; then
    OD=$1
fi

WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
cargo rr --bin launcher -- -o ${OD} --benchmark ./echo_services.toml --configfile ./phoenix_config.toml --timeout 60000
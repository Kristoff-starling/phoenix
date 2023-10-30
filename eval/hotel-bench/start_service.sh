#!/usr/bin/env bash
OD=/tmp/mrpc-eval
if [[ $# -ge 1 ]]; then
    OD=$1
fi

ssh root@h2 -p 2345 "rm /tmp/mrpc-eval -r"
ssh root@h3 -p 2346 "rm /tmp/mrpc-eval -r"
ssh root@h4 -p 2347 "rm /tmp/mrpc-eval -r"
ssh root@h5 -p 2348 "rm /tmp/mrpc-eval -r"
ssh root@h6 -p 2349 "rm /tmp/mrpc-eval -r"

WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
cargo rr --bin launcher -- -o ${OD} --benchmark ./hotel_services.toml --configfile ./phoenix_config.toml --timeout 6000

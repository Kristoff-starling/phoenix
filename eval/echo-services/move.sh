#!/bin/bash

SERVICES=("client2" "server")

for service in "${SERVICES[@]}"; do
    docker exec rpc_echo_$service rm -rf /root/phoenix/eval/echo-services
    docker cp ~/phoenix/eval/echo-services rpc_echo_$service:/root/phoenix/eval/echo-services
    docker cp ~/phoenix/experimental/mrpc/target/release/rpc_echo_$service rpc_echo_$service:/root/phoenix/target/phoenix/release/rpc_echo_$service
    docker cp ~/phoenix/experimental/mrpc/target/release/rpc_echo_$service.d rpc_echo_$service:/root/phoenix/target/phoenix/release/rpc_echo_$service.d
done
#!/bin/bash

docker exec rpc_echo_frontend rm -rf /root/phoenix/eval/echo-bench
docker cp ~/phoenix/eval/echo-bench rpc_echo_frontend:/root/phoenix/eval/echo-bench
docker cp ~/phoenix/experimental/mrpc/target/release/rpc_echo_frontend rpc_echo_frontend:/root/phoenix/target/phoenix/release/rpc_echo_frontend
docker cp ~/phoenix/experimental/mrpc/target/release/rpc_echo_frontend.d rpc_echo_frontend:/root/phoenix/target/phoenix/release/rpc_echo_frontend.d

SERVER="rpc-echo-server.c.app-defined-networks.internal"

ssh $SERVER docker exec rpc_echo_server rm -rf /root/phoenix/eval/echo-bench
rync -avz ~/phoenix/eval/echo-bench $SERVER:/tmp
rync -avz ~/phoenix/experimental/mrpc/target/release/rpc_echo_server $SERVER:/tmp
rync -avz ~/phoenix/experimental/mrpc/target/release/rpc_echo_server.d $SERVER:/tmp
ssh $SERVER docker cp /tmp/echo-bench rpc_echo_server:/root/phoenix/eval/echo-bench
ssh $SERVER docker cp /tmp/rpc_echo_server rpc_echo_server:/root/phoenix/target/phoenix/release/rpc_echo_server
ssh $SERVER docker cp /tmp/rpc_echo_server.d rpc_echo_server:/root/phoenix/target/phoenix/release/rpc_echo_server.d
ssh $SERVER rm -r /tmp/echo-bench
ssh $SERVER rm -r /tmp/rpc_echo_server
ssh $SERVER rm -r /tmp/rpc_echo_server.d
#!/bin/bash

FRONTEND_SERVER="h2"

ssh $FRONTEND_SERVER docker exec rpc_echo_frontend rm -rf /root/phoenix/eval/echo-bench
rsync -avz ~/phoenix/eval/echo-bench $FRONTEND_SERVER:/tmp
rsync -avz ~/phoenix/experimental/mrpc/target/release/rpc_echo_frontend $FRONTEND_SERVER:/tmp
rsync -avz ~/phoenix/experimental/mrpc/target/release/rpc_echo_frontend.d $FRONTEND_SERVER:/tmp
ssh $FRONTEND_SERVER docker cp /tmp/echo-bench rpc_echo_frontend:/root/phoenix/eval/echo-bench
ssh $FRONTEND_SERVER docker cp /tmp/rpc_echo_frontend rpc_echo_frontend:/root/phoenix/target/phoenix/release/rpc_echo_frontend
ssh $FRONTEND_SERVER docker cp /tmp/rpc_echo_frontend.d rpc_echo_frontend:/root/phoenix/target/phoenix/release/rpc_echo_frontend.d
ssh $FRONTEND_SERVER rm -r /tmp/echo-bench
ssh $FRONTEND_SERVER rm -r /tmp/rpc_echo_frontend
ssh $FRONTEND_SERVER rm -r /tmp/rpc_echo_frontend.d

SERVER_SERVER="h3"

ssh $SERVER_SERVER docker exec rpc_echo_server rm -rf /root/phoenix/eval/echo-bench
rsync -avz ~/phoenix/eval/echo-bench $SERVER_SERVER:/tmp
rsync -avz ~/phoenix/experimental/mrpc/target/release/rpc_echo_server $SERVER_SERVER:/tmp
rsync -avz ~/phoenix/experimental/mrpc/target/release/rpc_echo_server.d $SERVER_SERVER:/tmp
ssh $SERVER_SERVER docker cp /tmp/echo-bench rpc_echo_server:/root/phoenix/eval/echo-bench
ssh $SERVER_SERVER docker cp /tmp/rpc_echo_server rpc_echo_server:/root/phoenix/target/phoenix/release/rpc_echo_server
ssh $SERVER_SERVER docker cp /tmp/rpc_echo_server.d rpc_echo_server:/root/phoenix/target/phoenix/release/rpc_echo_server.d
ssh $SERVER_SERVER rm -r /tmp/echo-bench
ssh $SERVER_SERVER rm -r /tmp/rpc_echo_server
ssh $SERVER_SERVER rm -r /tmp/rpc_echo_server.d
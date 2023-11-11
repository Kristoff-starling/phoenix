#!/bin/bash

SERVICES=("frontend" "search" "geo" "rate" "profile")

for service in "${SERVICES[@]}"; do
    docker cp config.json hotel_$service:/root/phoenix/eval/hotel-services/config.json
    docker cp ~/phoenix/experimental/mrpc/exapmles/hotel_microservices/target/release/hotel_reserv_$service hotel_$service:/root/phoenix/target/phoenix/release/hotel_$service
    docker cp ~/phoenix/experimental/mrpc/exapmles/hotel_microservices/target/release/hotel_reserv_$service.d hotel_$service:/root/phoenix/target/phoenix/release/hotel_$service.d
done
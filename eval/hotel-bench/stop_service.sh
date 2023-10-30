#!/bin/bash
ssh root@h2 -p 2345 "ps -aux | grep hotel_reserv_frontend | awk '{print \$2}' | xargs kill -2"
ssh root@h3 -p 2346 "ps -aux | grep hotel_reserv_geo      | awk '{print \$2}' | xargs kill -2"
ssh root@h4 -p 2347 "ps -aux | grep hotel_reserv_profile  | awk '{print \$2}' | xargs kill -2"
ssh root@h5 -p 2348 "ps -aux | grep hotel_reserv_rate     | awk '{print \$2}' | xargs kill -2"
ssh root@h6 -p 2349 "ps -aux | grep hotel_reserv_search   | awk '{print \$2}' | xargs kill -2"
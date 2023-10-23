#!/bin/bash
rm statistics -r
mkdir statistics -p
ssh h2 "docker cp hotel_frontend:/tmp/mrpc-eval/microservices/frontend.csv ~/frontend.csv"
scp h2:~/frontend.csv statistics/frontend.csv
ssh h3 "docker cp hotel_geo:/tmp/mrpc-eval/microservices/geo.csv ~/geo.csv"
scp h3:~/geo.csv statistics/geo.csv
ssh h4 "docker cp hotel_profile:/tmp/mrpc-eval/microservices/profile.csv ~/profile.csv"
scp h4:~/profile.csv statistics/profile.csv
ssh h5 "docker cp hotel_rate:/tmp/mrpc-eval/microservices/rate.csv ~/rate.csv"
scp h5:~/rate.csv statistics/rate.csv
ssh h6 "docker cp hotel_search:/tmp/mrpc-eval/microservices/search.csv ~/search.csv"
scp h6:~/search.csv statistics/search.csv

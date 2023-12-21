#!/usr/bin/bash
WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
docker-compose -f docker-compose-frontend.yml -H "ssh://h2" up -d
docker-compose -f docker-compose-server.yml -H "ssh://h3" up -d
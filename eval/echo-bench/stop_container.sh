WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
docker-compose -f docker-compose-frontend.yml -H "ssh://rpc-echo-frontend.c.app-defined-networks.internal" down
docker-compose -f docker-compose-server.yml -H "ssh://rpc-echo-server.c.app-defined-networks.internal" down

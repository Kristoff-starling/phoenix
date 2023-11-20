WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
docker-compose -f docker-compose-services.yml up -d
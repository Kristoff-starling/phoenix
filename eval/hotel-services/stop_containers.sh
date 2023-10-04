WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
docker-compose -f docker-compose-profile.yml down
docker-compose -f docker-compose-geo.yml down 
docker-compose -f docker-compose-rate.yml down
docker-compose -f docker-compose-services.yml down

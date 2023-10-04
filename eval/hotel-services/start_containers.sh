WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
# docker-compose -f docker-compose-profile.yml up -d
# docker-compose -f docker-compose-geo.yml up -d
# docker-compose -f docker-compose-rate.yml up -d
docker-compose -f docker-compose-services.yml up -d

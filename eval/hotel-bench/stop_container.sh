WORKDIR=`dirname $(realpath $0)`
cd $WORKDIR
docker-compose -f docker-compose-frontend.yml -H "ssh://h2" down
docker-compose -f docker-compose-geo.yml -H "ssh://h3" down
docker-compose -f docker-compose-profile.yml -H "ssh://h4" down
docker-compose -f docker-compose-rate.yml -H "ssh://h5" down
docker-compose -f docker-compose-search.yml -H "ssh://h6" down
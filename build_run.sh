sudo rm -f build_scripts/logs/server-*-snapshot.json
docker compose -f ./build_scripts/docker-compose.shim.yml down && \
docker compose -f ./build_scripts/docker-compose.shim.yml build && \
docker compose -f ./build_scripts/docker-compose.shim.yml up -d && \
docker compose -f ./build_scripts/docker-compose.shim.yml logs -f
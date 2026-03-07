sudo rm -rf build_scripts/logs/server-*-snapshot.json build_scripts/logs/omnipaxos-node-*
docker compose -f ./build_scripts/docker-compose.shim.yml down && \
docker compose -f ./build_scripts/docker-compose.shim.yml build && \
docker compose -f ./build_scripts/docker-compose.shim.yml up -d && \
docker compose -f ./build_scripts/docker-compose.shim.yml logs -f
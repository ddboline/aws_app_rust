#!/bin/bash

PASSWORD=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 16; echo ;`
JWT_SECRET=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 32; echo ;`
SECRET_KEY=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 32; echo ;`
DB=aws_app_cache

docker run --name $DB -p 12345:5432 -e POSTGRES_PASSWORD=$PASSWORD -d postgres
sleep 10
DATABASE_URL="postgresql://postgres:$PASSWORD@localhost:12345/postgres"

psql $DATABASE_URL -c "CREATE DATABASE $DB"

DATABASE_URL="postgresql://postgres:$PASSWORD@localhost:12345/$DB"

mkdir -p ${HOME}/.config/aws_app_rust
cat > ${HOME}/.config/sync_app_rust/config.env <<EOL
DATABASE_URL=$DATABASE_URL
MY_OWNER_ID=8675309
DEFAULT_SECURITY_GROUP=sg-0
SPOT_SECURITY_GROUP=sg-0
DEFAULT_KEY_NAME=default-key
SCRIPT_DIRECTORY=~/
DOMAIN=localhost
JWT_SECRET=$JWT_SECRET
SECRET_KEY=$SECRET_KEY
EOL

#!/bin/bash

if [ -z "$PASSWORD" ]; then
    PASSWORD=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 16; echo ;`
fi
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
SECRET_PATH=${HOME}/.config/auth_server_rust/secret.bin
JWT_SECRET_PATH=${HOME}/.config/auth_server_rust/jwt_secret.bin
EOL

cat > ${HOME}/.config/aws_app_rust/postgres.toml <<EOL
[aws_app_rust]
database_url = 'postgresql://$USER:$PASSWORD@localhost:5432/$DB'
destination = 'file://$HOME/setup_files/build/aws_app_rust/backup'
tables = ['instance_family', 'instance_list', 'instance_pricing']
sequences = {instance_family_id_seq=['instance_family', 'id'], instance_pricing_id_seq=['instance_pricing', 'id']}
EOL

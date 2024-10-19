#!/bin/bash

if [ -z "$PASSWORD" ]; then
    PASSWORD=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 16; echo ;`
fi
DB=aws_app_cache

sudo apt-get install -y postgresql

sudo -u postgres createuser -E -e $USER
sudo -u postgres psql -c "CREATE ROLE $USER PASSWORD '$PASSWORD' NOSUPERUSER NOCREATEDB NOCREATEROLE INHERIT LOGIN;"
sudo -u postgres psql -c "ALTER ROLE $USER PASSWORD '$PASSWORD' NOSUPERUSER NOCREATEDB NOCREATEROLE INHERIT LOGIN;"
sudo -u postgres createdb $DB
sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE $DB TO $USER;"
sudo -u postgres psql $DB -c "GRANT ALL ON SCHEMA public TO $USER;"

mkdir -p ${HOME}/.config/aws_app_rust
cat > ${HOME}/.config/aws_app_rust/config.env <<EOL
DATABASE_URL=postgresql://$USER:$PASSWORD@localhost:5432/$DB
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

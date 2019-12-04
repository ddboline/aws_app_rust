#!/bin/bash

DB="aws_app_cache"
BUCKET="aws-app-rust-db-backup"

TABLES="
instance_family
instance_list
instance_pricing
"

mkdir -p backup/

for T in $TABLES;
do
    aws s3 cp s3://${BUCKET}/${T}.sql.gz backup/${T}.sql.gz
    gzip -dc backup/${T}.sql.gz | psql $DB -c "COPY $T FROM STDIN"
done

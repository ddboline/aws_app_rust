#!/bin/bash

DB="aws_app_cache"
BUCKET="aws-app-rust-db-backup"

TABLES="
instance_family
instance_list
instance_pricing
"

mkdir -p backup

for T in $TABLES;
do
    psql $DB -c "COPY $T TO STDOUT" | gzip > backup/${T}.sql.gz
    aws s3 cp backup/${T}.sql.gz s3://${BUCKET}/${T}.sql.gz
done

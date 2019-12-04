#!/bin/bash

DB="aws_app_cache"

TABLES="
instance_family
instance_list
instance_pricing
"

for T in $TABLES;
do
    psql $DB -c "DELETE FROM $T";
done

CREATE EXTENSION IF NOT EXISTS pgcrypto;

ALTER TABLE instance_pricing DROP COLUMN id;
ALTER TABLE instance_pricing ADD COLUMN id UUID PRIMARY KEY NOT NULL DEFAULT gen_random_uuid();

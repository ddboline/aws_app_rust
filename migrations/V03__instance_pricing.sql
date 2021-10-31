-- Your SQL goes here
CREATE TABLE instance_pricing (
    id SERIAL PRIMARY KEY,
    instance_type TEXT NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    price_type TEXT NOT NULL,
    price_timestamp TIMESTAMP WITH TIME ZONE NOT NULL
)

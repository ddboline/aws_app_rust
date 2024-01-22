CREATE TABLE inbound_email (
    id UUID PRIMARY KEY NOT NULL DEFAULT gen_random_uuid(),
    s3_bucket TEXT NOT NULL,
    s3_key TEXT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    subject TEXT NOT NULL,
    date TIMESTAMP WITH TIME ZONE NOT NULL,
    text_content TEXT NOT NULL,
    html_content TEXT NOT NULL,
    raw_email TEXT NOT NULL
)
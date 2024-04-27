CREATE TABLE dmarc_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    s3_key text,
    org_name text,
    email text,
    report_id text,
    date_range_begin integer,
    date_range_end integer,
    policy_domain text,
    source_ip text,
    count integer,
    auth_result_type text,
    auth_result_domain text,
    auth_result_result text,
    created_at timestamp with time zone
);
CREATE INDEX IF NOT EXISTS instance_family_family_type_idx ON instance_family (family_type);
CREATE INDEX IF NOT EXISTS instance_list_family_name_idx ON instance_list (family_name);
CREATE INDEX IF NOT ExISTS instance_pricing_instance_type_idx ON instance_pricing (instance_type);
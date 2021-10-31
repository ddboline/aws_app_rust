-- Your SQL goes here
CREATE TABLE instance_list (
    instance_type TEXT PRIMARY KEY,
    family_name TEXT NOT NULL,
    n_cpu INT NOT NULL,
    memory_gib DOUBLE PRECISION NOT NULL,
    generation TEXT NOT NULL,
    FOREIGN KEY (family_name) REFERENCES instance_family(family_name)
)

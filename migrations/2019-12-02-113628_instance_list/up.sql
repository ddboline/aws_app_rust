-- Your SQL goes here
CREATE TABLE instance_list (
    instance_type TEXT PRIMARY KEY,
    n_cpu INT NOT NULL,
    memory_gib DOUBLE PRECISION NOT NULL,
    generation TEXT NOT NULL
)

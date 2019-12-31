table! {
    authorized_users (email) {
        email -> Varchar,
        telegram_userid -> Nullable<Int8>,
    }
}

table! {
    instance_family (id) {
        id -> Int4,
        family_name -> Text,
        family_type -> Text,
    }
}

table! {
    instance_list (instance_type) {
        instance_type -> Text,
        n_cpu -> Int4,
        memory_gib -> Float8,
        generation -> Text,
    }
}

table! {
    instance_pricing (id) {
        id -> Int4,
        instance_type -> Text,
        price -> Float8,
        price_type -> Text,
        price_timestamp -> Timestamptz,
    }
}

allow_tables_to_appear_in_same_query!(
    authorized_users,
    instance_family,
    instance_list,
    instance_pricing,
);

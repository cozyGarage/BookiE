use sqlx::{MySql, Pool};

pub(crate) fn is_saturated(pool: &Pool<MySql>) -> bool {
    pool.size() >= pool.options().get_max_connections() && pool.num_idle() == 0
}

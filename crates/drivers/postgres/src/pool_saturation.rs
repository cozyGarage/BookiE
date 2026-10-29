use sqlx::PgPool;

pub(crate) fn is_saturated(pool: &PgPool) -> bool {
    pool.size() >= pool.options().get_max_connections() && pool.num_idle() == 0
}

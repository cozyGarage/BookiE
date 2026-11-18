The PostgreSQL release and MongoDB TLS test fixtures carry their seed scripts inside the image instead of bind-mounting them, so container CI jobs no longer start an unseeded database.

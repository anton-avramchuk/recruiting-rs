-- Реестр тенантов и платформенные администраторы. Не тенантные данные: без RLS.
CREATE TABLE tenants (
    id         uuid PRIMARY KEY,
    slug       text        NOT NULL UNIQUE,
    name       text        NOT NULL,
    status     text        NOT NULL CHECK (status IN ('provisioning', 'active', 'suspended')),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE platform_admins (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email         text        NOT NULL UNIQUE,
    password_hash text        NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- Локальное окружение: базы сервисов и непривилегированная роль приложения.
-- Миграции выполняются владельцем (postgres), сервисы работают от recruiting_app:
-- у неё нет BYPASSRLS, поэтому политики RLS действуют (см. docs/tenancy.md).
CREATE ROLE recruiting_app LOGIN PASSWORD 'recruiting_app' NOSUPERUSER NOBYPASSRLS;

CREATE DATABASE identity;
CREATE DATABASE admin;
CREATE DATABASE candidate;

\connect identity
GRANT USAGE ON SCHEMA public TO recruiting_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO recruiting_app;

\connect admin
GRANT USAGE ON SCHEMA public TO recruiting_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO recruiting_app;

\connect candidate
GRANT USAGE ON SCHEMA public TO recruiting_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO recruiting_app;

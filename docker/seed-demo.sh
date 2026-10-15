#!/bin/sh
# Создаёт демо-организацию «demo» с владельцем и тремя кандидатами через BFF (как это делает человек в интерфейсах).
# Идемпотентен: повторный запуск ничего не дублирует.
set -eu

ADMIN_BFF="${ADMIN_BFF:-http://bff-admin:8080}"
CRM_BFF="${CRM_BFF:-http://bff-crm:8080}"
ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-root@platform.io}"
ADMIN_PASSWORD="${SEED_ADMIN_PASSWORD:-root-password-1}"
OWNER_EMAIL="owner@demo.io"
OWNER_PASSWORD="demo-password-1"

csrf_of() { sed -n 's/.*"csrf_token":"\([^"]*\)".*/\1/p'; }

echo "1/4 вход администратора платформы"
ADMIN_CSRF=$(curl -fsS -c /tmp/admin.jar -H 'x-csrf-token: login' -H 'content-type: application/json' \
  -d "{\"email\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\"}" "$ADMIN_BFF/bff/login" | csrf_of)

echo "2/4 организация demo"
STATUS=$(curl -sS -o /tmp/tenant.out -w '%{http_code}' -b /tmp/admin.jar \
  -H "x-csrf-token: $ADMIN_CSRF" -H 'content-type: application/json' \
  -d "{\"slug\":\"demo\",\"name\":\"Demo\",\"owner\":{\"email\":\"$OWNER_EMAIL\",\"password\":\"$OWNER_PASSWORD\",\"display_name\":\"Владелец\"}}" \
  "$ADMIN_BFF/api/tenants")
case "$STATUS" in
  201) echo "  создана" ;;
  409) echo "  уже существует" ;;
  *) echo "  ошибка $STATUS: $(cat /tmp/tenant.out)"; exit 1 ;;
esac

echo "3/4 демо-кандидаты"
CRM_CSRF=$(curl -fsS -c /tmp/crm.jar -H 'x-csrf-token: login' -H 'content-type: application/json' \
  -d "{\"tenant\":\"demo\",\"email\":\"$OWNER_EMAIL\",\"password\":\"$OWNER_PASSWORD\"}" "$CRM_BFF/bff/login" | csrf_of)
add() {
  # без confirm_duplicate: уже существующий кандидат даст 409 и не задублируется
  code=$(curl -sS -o /dev/null -w '%{http_code}' -b /tmp/crm.jar -H "x-csrf-token: $CRM_CSRF" \
    -H 'content-type: application/json' -d "$1" "$CRM_BFF/api/persons")
  echo "  $2: $code"
}
add '{"full_name":"Анна Смирнова","email":"anna@example.com","phone":"+7 916 123-45-67","skills":["Rust","SQL"],"source":"hh.ru","expected_salary":300000,"declared_experience_years":6}' "Анна Смирнова"
add '{"full_name":"Борис Иванов","phone":"+7 903 555-66-77","skills":["Go"],"source":"Реферал","declared_experience_years":2}' "Борис Иванов"
add '{"full_name":"Виктор Петров","skills":["Rust","Docker"],"declared_experience_years":4}' "Виктор Петров"

echo "4/4 демо-вакансии и отклики"
post() {
  curl -sS -b /tmp/crm.jar -H "x-csrf-token: $CRM_CSRF" -H 'content-type: application/json' -d "$2" "$CRM_BFF$1"
}
if [ "$(curl -fsS -b /tmp/crm.jar "$CRM_BFF/api/vacancies")" = "[]" ]; then
  post /api/vacancies '{"title":"Senior Rust-разработчик","department":"Платформа","openings":2,"priority":"high","hiring_manager":"Иванов","recruiter":"Мария","required_skills":["Rust","SQL","Docker"],"open_days":52}' >/dev/null
  post /api/vacancies '{"title":"Go-разработчик","department":"Платформа","required_skills":["Go","SQL"],"open_days":12}' >/dev/null
fi
# отклики создаются, только пока их нет вообще (повторный запуск ничего не дублирует)
if curl -fsS -b /tmp/crm.jar "$CRM_BFF/api/applications?page_size=1" | grep -q '"total":0'; then
  VACANCIES=$(curl -fsS -b /tmp/crm.jar "$CRM_BFF/api/vacancies" | tr '{' '
')
  PEOPLE=$(curl -fsS -b /tmp/crm.jar "$CRM_BFF/api/persons?page_size=100" | tr '{' '
')
  vacancy() { printf '%s' "$VACANCIES" | sed -n "s/.*\"id\":\"\([^\"]*\)\",\"title\":\"$1\".*/\1/p" | head -n 1; }
  person() { printf '%s' "$PEOPLE" | sed -n "s/.*\"id\":\"\([^\"]*\)\",\"full_name\":\"$1\".*/\1/p" | head -n 1; }
  apply() { # кандидат, вакансия, этап
    code=$(curl -sS -o /dev/null -w '%{http_code}' -b /tmp/crm.jar -H "x-csrf-token: $CRM_CSRF" \
      -H 'content-type: application/json' \
      -d "{\"person_id\":\"$(person "$1")\",\"vacancy_id\":\"$(vacancy "$2")\",\"stage\":\"$3\"}" "$CRM_BFF/api/applications")
    echo "  $1: $code"
  }
  apply "Анна Смирнова" "Senior Rust-разработчик" screening
  apply "Борис Иванов" "Go-разработчик" new
  apply "Виктор Петров" "Senior Rust-разработчик" new
  apply "Анна Смирнова" "Go-разработчик" hr
else
  echo "  отклики уже есть"
fi
echo "готово"

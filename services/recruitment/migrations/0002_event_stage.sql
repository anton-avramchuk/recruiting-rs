-- Этап в событиях перевода и отказа: аналитика считает воронку по пройденным этапам,
-- а не по текущему.
ALTER TABLE application_events
    ADD COLUMN stage text
    CHECK (stage IS NULL OR stage IN ('new', 'screening', 'hr', 'tech', 'final', 'offer', 'hired', 'rejected'));

-- Существующие записи: этап берётся из текста «Этап → <название>».
UPDATE application_events SET stage = CASE text
    WHEN 'Этап → Новый' THEN 'new'
    WHEN 'Этап → Скрининг' THEN 'screening'
    WHEN 'Этап → Интервью HR' THEN 'hr'
    WHEN 'Этап → Тех. интервью' THEN 'tech'
    WHEN 'Этап → Финал' THEN 'final'
    WHEN 'Этап → Оффер' THEN 'offer'
    WHEN 'Этап → Выход' THEN 'hired'
    WHEN 'Этап → Отказ' THEN 'rejected'
END
WHERE kind = 'stage';

UPDATE application_events SET stage = 'rejected' WHERE kind = 'rejected';

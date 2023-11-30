DROP TABLE lesson;
CREATE TABLE lesson (
    id             SERIAL PRIMARY KEY,
    name           VARCHAR(512) NOT NULL,
    teacher_code   VARCHAR(128) NOT NULL,
    student_code   VARCHAR(128) NOT NULL,
    digital_human_list TEXT,
    language_list VARCHAR(256) NOT NULL DEFAULT '',
    start_time     INTEGER NOT NULL DEFAULT 0,
    end_time       INTEGER NOT NULL DEFAULT 0,
    create_time    TIMESTAMP NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX idx_name ON lesson(name);

DROP TABLE member;
CREATE TABLE member (
    id              SERIAL PRIMARY KEY,
    lesson_id       INTEGER NOT NULL,
    identifier      TEXT,
    is_teacher      INTEGER NOT NULL DEFAULT 0,
    avatar_url      VARCHAR(256) NOT NULL,
    nickname        VARCHAR(512) NOT NULL,
    language        VARCHAR(32) NOT NULL,
    create_time     TIMESTAMP NULL DEFAULT CURRENT_TIMESTAMP
);

DROP TABLE digital_human;
CREATE TABLE digital_human (
    id SERIAL PRIMARY KEY,
    avatar_url VARCHAR(256) NOT NULL,
    create_time    TIMESTAMP NULL DEFAULT CURRENT_TIMESTAMP
);
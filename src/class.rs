use aws_config::default_provider::use_fips::use_fips_provider;
use crate::Context;
use chrono::prelude::*;
use poem::web::{Data, Json, RemoteAddr};
use poem::Request;
use poem::{handler, IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_postgres::Client;

#[handler]
pub(crate) async fn supported_lang_list(ctx: Data<&Context>) -> String {
    json!({
        "status": "success",
        "data": ["Arabic", "English", "Chinese", "Hindi", "Bangladesh", "Urdu"]
    }).to_string()
}

#[handler]
pub(crate) async fn digital_human_list(ctx: Data<&Context>) -> String {
    match ctx
        .pg_client
        .query("SELECT avatar_url FROM digital_human", &[])
        .await
    {
        Err(e) => json!({
            "status": "failed",
            "error": format!("{}", e)
        })
        .to_string(),
        Ok(rows) => {
            let urls: Vec<String> = rows
                .into_iter()
                .map(|x| x.get::<usize, String>(0))
                .collect();
            json!({
                "status": "success",
                "data": {
                    "avatar_urls": urls,
                },
            })
            .to_string()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateClassReq {
    name: String,
    start_time: i64,
    end_time: i64,
    digital_human_list: Vec<String>,
    supported_lang: Vec<String>,
    teacher_code: String,
    student_code: Vec<String>,
}

#[handler]
pub(crate) async fn create_lesson(ctx: Data<&Context>, req: Json<CreateClassReq>) -> String {
    let digital_humans: String = req.digital_human_list.join(",");
    let language_list: String = req.supported_lang.join(",");
    match ctx.pg_client.execute("INSERT INTO lesson (name, digital_human_list, language_list, start_time, end_time)VALUES (?, ?, ?, ?, ?)",
                          &[&req.name, &digital_humans, &language_list, &req.start_time, &req.end_time]).await {
        Err(e) => {
            json!({
                "status": "failed",
                "error": format!("{}", e)
            }).to_string()
        }
        Ok(e) => {
            json!({
                "status": "success"
            }).to_string()
        }
    }
}

#[derive(Serialize)]
pub struct Lesson {
    id: u64,
    name: String,
    digital_human_list: Vec<String>,
    supported_language: Vec<String>,
    start_time: String,
    end_time: String,
    create_time: String,
    teacher_link: String,
    student_link: String,
    teacher_code: String,
    student_code: String,
    status: String,
}

#[handler]
pub(crate) async fn lesson_list(ctx: Data<&Context>) -> String {
    match ctx.pg_client.query("SELECT (id, name, digital_human_list, language_list, start_time, end_time, teacher_code, student_code, create_time) FROM lesson", &[]).await {
        Err(e) => {
            json!({
                "status": "failed",
                "error": format!("{}", e)
            }).to_string()
        }
        Ok(rows) => {
            let lessons: Vec<Lesson> = rows.into_iter().map(|row| -> Lesson {
                let start_time = row.get(5);
                let end_time = row.get(6);
                let now = Local::now().timestamp();
                let status = if start_time > now {
                    "Not Started"
                } else if start_time < now && now < end_time {
                    "Teaching"
                } else {
                    "Ended"
                }.to_string();
                Lesson {
                    id: row.get::<usize, i64>(0) as u64,
                    name: row.get(1),
                    digital_human_list: row.get::<usize, String>(2).split(",").map(|x| x.to_string()).collect(),
                    supported_language: row.get::<usize, String>(3).split(",").map(|x| x.to_string()).collect(),
                    start_time: NaiveDateTime::from_timestamp_millis(start_time).unwrap().format("%Y-%m-%d %H:%M:%S").to_string(),
                    end_time: NaiveDateTime::from_timestamp_millis(end_time).unwrap().format("%Y-%m-%d %H:%M:%S").to_string(),
                    teacher_code: row.get(7),
                    student_code: row.get(8),
                    teacher_link: "".to_string(),
                    student_link: "".to_string(),
                    create_time: row.get(9),
                    status
                }
            }).collect();
            json!({
                "status": "success",
                "data": {
                    "lessons": lessons
                }
            }).to_string()
        }
    }
}

#[derive(Deserialize)]
pub struct EnterClassReq {
    lesson_id: i64,
    is_teacher: i64,
    avatar_url: String,
    nickname: String,
    language: String,
}

#[handler]
pub(crate) async fn enter_class(
    ctx: Data<&Context>,
    request: &Request,
    remote_addr: &RemoteAddr,
    req: Json<EnterClassReq>,
) -> String {
    let identifier = match request.header("User-Agent") {
        None => return json!({"status": "failed", "error": "User-Agent required"}).to_string(),
        Some(ua) => format!("{}-{}", ua.to_string(), remote_addr.to_string()),
    };
    match ctx.pg_client.execute("INSERT INTO member (lesson_id, identifier, is_teacher, avatar_url, nickname, language)VALUES(?, ?, ?, ?, ?, ?)",
        &[&req.lesson_id, &identifier, &req.is_teacher, &req.avatar_url, &req.nickname, &req.language]).await {
        Err(e) => json!({"status": "failed", "error": format!("{}", e)}),
        Ok(_) => json!({"status":"success"})
    }.to_string()
}

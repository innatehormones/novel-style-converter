use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{ModelConfig, NewModelConfig};

pub struct ModelConfigRepo<'a> { pub(crate) conn: &'a Connection }

impl<'a> ModelConfigRepo<'a> {
    pub fn insert(&self, m: &NewModelConfig) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO model_configs \
             (name, base_url, api_key, model, max_tokens, temperature, concurrency) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                m.name, m.base_url, m.api_key, m.model,
                m.max_tokens, m.temperature, m.concurrency,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list(&self) -> Result<Vec<ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, base_url, api_key, model, max_tokens, temperature, concurrency \
             FROM model_configs ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, base_url, api_key, model, max_tokens, temperature, concurrency \
             FROM model_configs WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn update(&self, m: &ModelConfig) -> Result<()> {
        self.conn.execute(
            "UPDATE model_configs SET \
             name=?2, base_url=?3, api_key=?4, model=?5, \
             max_tokens=?6, temperature=?7, concurrency=?8 \
             WHERE id=?1",
            params![
                m.id, m.name, m.base_url, m.api_key, m.model,
                m.max_tokens, m.temperature, m.concurrency,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM model_configs WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 表为空时插入一条种子记录;表非空(已有用户配置)时跳过,不报错不覆盖。
    pub fn seed_default_if_empty(&self, seed: &NewModelConfig) -> Result<Option<i64>> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM model_configs", [], |r| r.get(0))?;
        if n > 0 { return Ok(None); }
        let id = self.insert(seed)?;
        Ok(Some(id))
    }
}

fn from_row(row: &Row) -> rusqlite::Result<ModelConfig> {
    Ok(ModelConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        api_key: row.get(3)?,
        model: row.get(4)?,
        max_tokens: row.get(5)?,
        temperature: row.get(6)?,
        concurrency: row.get(7)?,
    })
}

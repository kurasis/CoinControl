use crate::{AssetPolicy, Result, Store, StoreError};
use sqlx::Row;

impl Store {
    pub async fn list_asset_policies(&self) -> Result<Vec<AssetPolicy>> {
        Ok(sqlx::query("SELECT a.id,a.symbol,a.name,a.verification,COALESCE(p.hidden,0) AS hidden,p.exclude_override FROM assets a LEFT JOIN asset_preferences p ON p.asset_id=a.id ORDER BY a.network_id,a.id")
            .fetch_all(&self.pool).await?.iter().map(|r| AssetPolicy {
                asset_id:r.get("id"),symbol:r.get("symbol"),name:r.get("name"),verification:r.get("verification"),
                hidden:r.get::<i64,_>("hidden")!=0,exclude_override:r.get::<Option<i64>,_>("exclude_override").map(|v|v!=0),
            }).collect())
    }
    pub async fn set_asset_policy(
        &self,
        asset_id: &str,
        hidden: bool,
        exclude_override: Option<bool>,
    ) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assets WHERE id=?)")
            .bind(asset_id)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            return Err(StoreError::NotFound("asset"));
        }
        sqlx::query("INSERT INTO asset_preferences(asset_id,hidden,exclude_override) VALUES(?,?,?) ON CONFLICT(asset_id) DO UPDATE SET hidden=excluded.hidden,exclude_override=excluded.exclude_override")
            .bind(asset_id).bind(i64::from(hidden)).bind(exclude_override.map(i64::from)).execute(&mut *tx).await?;
        crate::ingest::mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

use super::*;
use crate::{
    model::{
        nfo::{episode::Episode, tvshow::Tvshow, Nfo, ProviderKnown},
        setting,
    },
    utils::file,
};
use std::fmt::Debug;

pub async fn process<P: AsRef<Path>>(
    id: &str,
    provider: ProviderKnown,
    title: &str,
    lang: &str,
    season: u64,
    episode: u64,
    path: P,
) -> Result<(), TvshowNfoServiceError> {
    let path = path.as_ref();
    log::info!("Processing {:?}", path);

    let tvshow_title = title.clone();
    let tvshow_folder = make_vaild_pathname(&tvshow_title);
    let tvshow_path = setting::Setting::get_tvshow_repository_path().join(&tvshow_folder);
    let tvshow_nfo_path = tvshow_path.join("tvshow.nfo");

    let mut tvshow_nfo: Tvshow;
    if tvshow_nfo_path.exists() {
        tvshow_nfo = read_nfo(&tvshow_nfo_path)?;
    } else {
        tvshow_nfo = Tvshow::new(&id, provider.clone().into());
    }
    // 从网络Tvshow获取信息
    if let Err(e) = tvshow_nfo.update(lang).await {
        log::error!("Get {} tvshow nfo error: {:?}", tvshow_title, e);
        return Err(TvshowNfoServiceError::NetworkError(e));
    }

    write_nfo(&tvshow_nfo_path, &tvshow_nfo)?;

    // multi-thread download
    let mut donwload_pool = Vec::new();

    for (path, thumb) in tvshow_nfo.get_thumb(&tvshow_path) {
        donwload_pool.push(tokio::spawn(
            async move { download_thumb(&path, &thumb).await },
        ));
    }

    for task in donwload_pool {
        let _ = task.await;
    }

    // 从网络Episode获取信息
    let mut episode_nfo = Episode::new(&id, provider.clone().into());
    if let Err(e) = episode_nfo
        .update(lang, season, episode, &tvshow_nfo.get_fallback_lang())
        .await
    {
        log::error!(
            "Get {} S{:02}E{:02} nfo error: {:?}",
            tvshow_title,
            season,
            episode,
            e
        );
        return Err(TvshowNfoServiceError::NetworkError(e));
    }

    let episode_file_title = make_vaild_pathname(&episode_nfo.title);
    let episode_folder_path = tvshow_path.join(if season == 0 {
        "Specials".to_string()
    } else {
        format!("Season {:02}", season)
    });
    let episode_nfo_path = episode_folder_path.join(format!(
        "{} - S{:02}E{:02} - {}.nfo",
        &tvshow_folder, season, episode, &episode_file_title
    ));
    let episode_path = episode_folder_path.join(format!(
        "{} - S{:02}E{:02} - {}.{}",
        &tvshow_folder,
        season,
        episode,
        &episode_file_title,
        path.extension().unwrap().to_str().unwrap()
    ));

    file::move_video_file_with_queue(path.to_path_buf(), episode_path);

    write_nfo(&episode_nfo_path, &episode_nfo)?;
    if let Some(thumb) = episode_nfo.get_thumb() {
        download_thumb(
            episode_folder_path.join(format!(
                "{} - S{:02}E{:02} - {}-thumb{}",
                &tvshow_folder, season, episode, &episode_file_title, &thumb.1
            )),
            thumb.0,
        )
        .await?;
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum TvshowNfoServiceError {
    #[error(transparent)]
    RegexBuildError(#[from] crate::utils::matcher::MatcherError),
    #[error(transparent)]
    NfoCreateError(#[from] NfoServiceError),
    #[error(transparent)]
    SledError(#[from] crate::data::subscribe_rules::SubscribeRulesDataError),
    #[error(transparent)]
    NetworkError(#[from] crate::model::nfo::NfoGetError),
}

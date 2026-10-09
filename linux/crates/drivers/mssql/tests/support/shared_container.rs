use secrecy::SecretString;
use tablepro_core::ConnectOptions;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::core::{ExecCommand, Mount};
use testcontainers_modules::mssql_server::MssqlServer;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

pub(crate) async fn start_mssql() -> (ContainerAsync<MssqlServer>, ConnectOptions) {
    start_mssql_with_durability(false).await
}

pub(crate) async fn start_mssql_durable() -> (ContainerAsync<MssqlServer>, ConnectOptions) {
    start_mssql_with_durability(true).await
}

async fn start_mssql_with_durability(durable: bool) -> (ContainerAsync<MssqlServer>, ConnectOptions) {
    let image = MssqlServer::default()
        .with_accept_eula()
        .with_startup_timeout(std::time::Duration::from_secs(180));
    let container = if durable {
        image.start().await
    } else {
        image
            .with_mount(Mount::tmpfs_mount("/var/opt/mssql").with_mode(0o1777))
            .start()
            .await
    }
    .expect("start mssql container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(1433).await.expect("port"),
        database: "master".into(),
        username: "sa".into(),
        password: SecretString::new(MssqlServer::DEFAULT_SA_PASSWORD.to_string().into()),
        tls: tablepro_core::TlsConfig::disabled(),
        ..Default::default()
    };
    (container, options)
}

pub(crate) async fn assert_tmpfs_data_dir(container: &ContainerAsync<MssqlServer>) {
    let mut result = container.exec(ExecCommand::new(["cat", "/proc/mounts"])).await.unwrap();
    let mounts = String::from_utf8(result.stdout_to_vec().await.unwrap()).unwrap();
    assert!(mounts.lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.get(1) == Some(&"/var/opt/mssql") && fields.get(2) == Some(&"tmpfs")
    }));
}

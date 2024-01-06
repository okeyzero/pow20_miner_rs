use std::ops::Div;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dotenv::dotenv;
use ethers::core::k256::ecdsa::SigningKey;
use ethers::prelude::*;
use ethers::utils::{hex, keccak256};
use indicatif::{ProgressBar, ProgressStyle};
use log::{info, warn};
use rayon::prelude::*;
use serde::Deserialize;
use tokio;
use tokio::time::{Duration, interval};

use crate::initialization::{log_banner, print_banner, setup_logger};

mod initialization;

static TIMES: AtomicUsize = AtomicUsize::new(0);

abigen!(
    IPOW,
    r#"[
        function mine(uint256 id, uint256 amount, uint nonce) public payable
        function getLastTokenId(uint256 id) public view returns (uint256)
        function getLastTokenHash() public view returns (bytes32)
        function validateNonce(uint256 id, uint256 nonce) public view returns (bool)
        function getInscription(uint256 id) public view returns (string memory)
        function getCollectionPaused(uint256 id) public view returns (bool)
        function balanceOf(address account, uint256 id) public view returns (uint256)
        function totalSupply(uint256 id) public view returns (uint256)
    ]"#,
);
#[derive(Deserialize, Debug)]
pub struct Config {
    pub rpc_url: String,
    pub private_key: String,
    pub count: u32,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    setup_logger()?;
    print_banner();

    info!("开始执行任务");
    warn!("🐦 Twitter:[𝕏] @0xNaiXi");
    warn!("🐦 Twitter:[𝕏] @0xNaiXi");
    warn!("🐦 Twitter:[𝕏] @0xNaiXi");
    warn!("🐙 GitHub URL: https://github.com/okeyzero");
    // 解析 .env 文件
    let config = envy::from_env::<Config>()?;
    let provider = Provider::<Http>::try_from(&config.rpc_url)?;
    let chain_id = provider.get_chainid().await?;
    let private_key = config.private_key.clone();
    let wallet = private_key
        .parse::<LocalWallet>()
        .unwrap()
        .with_chain_id(chain_id.as_u64());
    let address = wallet.address();
    let nonce = provider.get_transaction_count(address, None).await?;
    info!("🏅 当前钱包地址: {:?}", address);
    info!("🏅 当前链ID: {:?}", chain_id);
    info!("🏅 钱包nonce: {:?}", nonce);

    let provider = Arc::new(SignerMiddleware::new(provider, wallet));
    let contract_address = "0x550B0ac1E89b10eC6969b777FDcA4791Ed131079";
    let contract_address: Address = contract_address.parse()?;
    let contract = Arc::new(IPOW::new(contract_address, provider.clone()));

    let mut success = 0;

    let speed_bar = ProgressBar::new(100);
    speed_bar.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold} {spinner:.green} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    speed_bar.set_prefix("🚄 Speed");
    let mut interval = interval(Duration::from_secs(1));
    let mut max_speed = 0.0;
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            let total_hash_count = TIMES.swap(0, Ordering::Relaxed);
            let hashes_per_second = total_hash_count as f64 / 1000.0;
            if hashes_per_second > max_speed {
                max_speed = hashes_per_second;
            }
            speed_bar.set_message(format!("Hash per second: {:.2} K/s - max speed: {:.2} K/s", hashes_per_second, max_speed));
        }
    });


    while success < config.count {
        log_banner(format!("第 {} 次挖矿,共 {} 次", success + 1, config.count));
        if miner(&contract, address).await? {
            success = success + 1;
        }
    }

    info!("🏆 任务执行完毕");

    //编译成exe 取消下面的屏蔽 不让程序关闭窗口 不然的话 会执行完任务 直接关闭窗口 无法看输出的日志了
    //tokio::time::sleep(Duration::new(1000, 0)).await;
    Ok(())
}

async fn miner(contract: &Arc<IPOW<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>>, address: Address) -> Result<bool, Box<dyn std::error::Error>> {
    let id = U256::from(1);
    let revenue = U256::from(1000);
    let balance = contract.balance_of(address, id).call().await?;
    info!("🏅 balance: {:?}", balance);
    let supply = contract.total_supply(id).call().await?;
    info!("🏅 Total supply: {:?}", supply);
    let last_token_id = contract.get_last_token_id(id).call().await?;
    info!("🏅 getLastTokenId: {:?}", last_token_id);
    let last_token_hash = contract.get_last_token_hash().call().await?;
    info!("🏅 getLastTokenHash: {:?}", hex::encode_prefixed(&last_token_hash));

    let difficulty = U256::from("0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").div(U256::from(36000));

    info!("⛰️  Difficulty: {}", difficulty);
    let target = get_target(last_token_id, difficulty);
    info!("🎯 Target: {}", target);

    let nonce = mine_worker(address.clone(), last_token_hash, target);

    if let Some(nonce) = nonce {
        info!("✅  Find the nonce: {}", nonce);
        let result = contract.mine(id, revenue, nonce).send().await.unwrap().await.unwrap();
        match result {
            Some(tx) => {
                info!("🙆 Successfully mined a block: {:?}", tx.transaction_hash);
            }
            None => {
                info!("⚠️ Failed to mine a block");
            }
        }
    } else {
        return Ok(false);
    }
    Ok(true)
}

fn get_target(e: U256, difficulty: U256) -> U256 {
    let zle = U256::from(2);
    if e.is_zero() {
        return difficulty;
    }
    let t = U256::from(e.as_usize().ilog10());
    let n = difficulty.div(zle.pow(t));
    if n < U256::from(240000) {
        U256::from(240000)
    } else {
        n
    }
}


fn mine_worker(
    from: Address,
    challenge: [u8; 32],
    target: U256,
) -> Option<U256> {
    let base_nonce = U256::from(0);
    let challenge_bytes = challenge.clone();
    let from_bytes = from.as_bytes();
    (0..u64::MAX)
        .into_par_iter()
        .map(|index| {
            TIMES.fetch_add(1, Ordering::Relaxed);
            let nonce = base_nonce + U256::from(index);
            let mut data = Vec::new();
            data.extend_from_slice(&from_bytes);
            data.extend_from_slice(&challenge_bytes);

            let nonce_bytes = {
                let mut buf = [0u8; 32];
                nonce.to_big_endian(&mut buf);
                buf
            };
            data.extend_from_slice(&nonce_bytes);
            let hash = keccak256(&data);
            let hash_val = U256::from_big_endian(&hash);
            if hash_val < target {
                info!("🎯 Nonce {} hash_val: {} target: {}", nonce, hash_val, target);
                Some(nonce)
            } else {
                None
            }
        })
        .find_any(|result| result.is_some())
        .flatten()
}

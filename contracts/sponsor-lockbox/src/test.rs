#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup<'a>() -> (Env, Address, SponsorLockboxClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(SponsorLockbox, ());
    let client = SponsorLockboxClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    (env, admin, client)
}

#[test]
fn liability_snapshot_tracks_release_and_cancel() {
    let (env, admin, client) = setup();
    let sponsor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let beneficiary_two = Address::generate(&env);

    client.register_lock(&admin, &1u64, &sponsor, &beneficiary, &100i128, &2_000u64);
    client.register_lock(
        &admin,
        &2u64,
        &sponsor,
        &beneficiary_two,
        &50i128,
        &4_000u64,
    );

    let before = client.liability_snapshot();
    assert_eq!(before.active_count, 2);
    assert_eq!(before.active_amount, 150);
    assert_eq!(before.releasable_count, 0);

    env.ledger().set_timestamp(2_500);
    let queue = client.unlock_queue_accessor();
    assert_eq!(queue.releasable_count, 1);
    assert_eq!(queue.pending_count, 1);
    assert_eq!(queue.next_unlock_at, 4_000);

    client.release(&beneficiary, &1u64);
    client.cancel(&admin, &2u64);
    let after = client.liability_snapshot();
    assert_eq!(after.active_count, 0);
    assert_eq!(after.released_count, 1);
    assert_eq!(after.cancelled_count, 1);
}

#[test]
fn unlock_queue_accessor_reports_empty_defaults_when_unconfigured() {
    let env = Env::default();
    let contract_id = env.register(SponsorLockbox, ());
    let client = SponsorLockboxClient::new(&env, &contract_id);

    let queue = client.unlock_queue_accessor();
    assert!(!queue.configured);
    assert_eq!(queue.indexed_locks, 0);
    assert_eq!(queue.pending_count, 0);
}

#[test]
fn test_release_and_cancel_mutually_exclusive() {
    let (env, admin, client) = setup();
    let sponsor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let imposter = Address::generate(&env);

    client.register_lock(&admin, &10u64, &sponsor, &beneficiary, &100i128, &2_000u64);
    client.register_lock(&admin, &20u64, &sponsor, &beneficiary, &200i128, &2_000u64);

    // Release before unlock time fails
    env.ledger().set_timestamp(1_500);
    let err_early = client.try_release(&beneficiary, &10u64);
    assert_eq!(err_early, Err(Ok(Error::NotUnlockedYet)));

    // Unauthorized beneficiary fails
    env.ledger().set_timestamp(2_500);
    let err_auth = client.try_release(&imposter, &10u64);
    assert_eq!(err_auth, Err(Ok(Error::NotAuthorized)));

    // Valid release
    let res = client.release(&beneficiary, &10u64);
    assert_eq!(res, 100);

    // Cancel after release fails
    let err_cancel = client.try_cancel(&admin, &10u64);
    assert_eq!(err_cancel, Err(Ok(Error::AlreadyReleased)));

    // Cancel lock 20
    assert!(client.try_cancel(&admin, &20u64).is_ok());

    // Release after cancel fails
    let err_rel = client.try_release(&beneficiary, &20u64);
    assert_eq!(err_rel, Err(Ok(Error::AlreadyCancelled)));
}

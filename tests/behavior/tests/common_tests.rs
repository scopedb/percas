use behavior_tests::Testkit;
use behavior_tests::harness;
use test_harness::test;

#[test(harness)]
async fn test_put_get(testkit: Testkit) {
    testkit.client.put("key", "value".as_bytes()).await.unwrap();
    let value = testkit.client.get("key").await.unwrap();
    assert_eq!(value.unwrap(), b"value");
}

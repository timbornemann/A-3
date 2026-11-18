//! Pure metadata helpers; genuine full historical replay is covered by native cold activation.
use super::*;

#[test]
fn prior_intent_is_explicitly_unverified_and_retained_verbatim() -> Result<(), Box<dyn Error>> {
    let intent =
        HttpCorrectionPlanIntent::try_from_string("Change only the selected handler".to_owned())?;
    let mut text = String::new();
    append_unverified_intent(&mut text, &intent);
    assert_eq!(
        text,
        "Prior model intent (unverified; no fact or proof): Change only the selected handler\n"
    );
    Ok(())
}

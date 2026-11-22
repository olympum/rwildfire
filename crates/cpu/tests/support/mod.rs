pub fn flame(spherical: bool) -> String {
    let extra = if spherical {
        " spherical3D=\"0.1\""
    } else {
        ""
    };
    let palette = "E0A080".repeat(256);
    format!(
        r#"<flame size="40 30" center="0 0" scale="12" brightness="4">
<xform weight="1" color="0.3" coefs="0.5 0 0 0.5 -0.65 -0.5" linear3D="1"{extra}/>
<xform weight="1" color="0.6" coefs="0.5 0 0 0.5 0.65 -0.5" linear3D="1"/>
<xform weight="1" color="0.9" coefs="0.5 0 0 0.5 0 0.65" linear3D="1"/>
<palette count="256" format="RGB">{palette}</palette>
</flame>"#
    )
}

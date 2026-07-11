#[cfg(test)]
use super::*;

mod cases {
    use super::super::helpers::premultiply_rgba8;
    use super::*;
    use crate::render::live2d::model::Live2DLayout;

    #[test]
    fn premultiply_rgba8_matches_expected_rounding() {
        let mut bytes = [200, 100, 50, 128];
        premultiply_rgba8(&mut bytes);
        assert_eq!(bytes, [100, 50, 25, 128]);
    }

    #[test]
    fn parse_layout_accepts_model3_keys() {
        let json = serde_json::json!({
            "Layout": {
                "CenterX": 0.25,
                "Bottom": -1.1,
                "Width": 1.8
            }
        });
        let layout = Live2DModelResource::parse_layout(&json).expect("layout should parse");
        assert_eq!(
            layout,
            Live2DLayout {
                width: Some(1.8),
                bottom: Some(-1.1),
                center_x: Some(0.25),
                ..Default::default()
            }
        );
    }

    #[test]
    fn parse_user_data_resolves_artmesh_entries() {
        let text = serde_json::json!({
            "UserData": [
                {
                    "Target": "ArtMesh",
                    "Id": "DrawableA",
                    "Value": "head"
                },
                {
                    "Target": "Part",
                    "Id": "PartA",
                    "Value": "group"
                }
            ]
        })
        .to_string();

        let parsed = Live2DModelResource::parse_user_data_with_resolver(&text, |id| match id {
            "DrawableA" => Some(7),
            _ => None,
        })
        .expect("userdata should parse");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].drawable_index, Some(7));
        assert_eq!(parsed[0].value, "head");
        assert_eq!(parsed[1].drawable_index, None);
    }

    #[test]
    fn parse_display_info_reads_all_named_sections() {
        let text = serde_json::json!({
            "Parameters": [
                {
                    "Id": "ParamAngleX",
                    "GroupId": "Face",
                    "Name": "Angle X"
                }
            ],
            "ParameterGroups": [
                {
                    "Id": "Face",
                    "GroupId": "",
                    "Name": "Face Group"
                }
            ],
            "Parts": [
                {
                    "Id": "PartBody",
                    "Name": "Body"
                }
            ]
        })
        .to_string();

        let parsed =
            Live2DModelResource::parse_display_info(&text).expect("display info should parse");

        assert_eq!(parsed.parameters.len(), 1);
        assert_eq!(parsed.parameters[0].id, "ParamAngleX");
        assert_eq!(parsed.parameters[0].group_id, "Face");
        assert_eq!(parsed.parameter_groups[0].name, "Face Group");
        assert_eq!(parsed.parts[0].id, "PartBody");
        assert_eq!(parsed.parts[0].group_id, "");
    }
}

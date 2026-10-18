//! Unity's built-in type-tree string table (UnityPy's TPK, Unity 6), addressed by blob string
//! offsets with the top bit set. Later Unity versions only append, so offsets stay stable.

const BUFFER: &str = "AABB\0AnimationClip\0AnimationCurve\0AnimationState\0Array\0Base\0BitField\0\
bitset\0bool\0char\0ColorRGBA\0Component\0data\0deque\0double\0dynamic_array\0FastPropertyName\0\
first\0float\0Font\0GameObject\0Generic Mono\0GradientNEW\0GUID\0GUIStyle\0int\0list\0long long\0\
map\0Matrix4x4f\0MdFour\0MonoBehaviour\0MonoScript\0m_ByteSize\0m_Curve\0m_EditorClassIdentifier\0\
m_EditorHideFlags\0m_Enabled\0m_ExtensionPtr\0m_GameObject\0m_Index\0m_IsArray\0m_IsStatic\0\
m_MetaFlag\0m_Name\0m_ObjectHideFlags\0m_PrefabInternal\0m_PrefabParentObject\0m_Script\0\
m_StaticEditorFlags\0m_Type\0m_Version\0Object\0pair\0PPtr<Component>\0PPtr<GameObject>\0\
PPtr<Material>\0PPtr<MonoBehaviour>\0PPtr<MonoScript>\0PPtr<Object>\0PPtr<Prefab>\0PPtr<Sprite>\0\
PPtr<TextAsset>\0PPtr<Texture>\0PPtr<Texture2D>\0PPtr<Transform>\0Prefab\0Quaternionf\0Rectf\0\
RectInt\0RectOffset\0second\0set\0short\0size\0SInt16\0SInt32\0SInt64\0SInt8\0staticvector\0string\0\
TextAsset\0TextMesh\0Texture\0Texture2D\0Transform\0TypelessData\0UInt16\0UInt32\0UInt64\0UInt8\0\
unsigned int\0unsigned long long\0unsigned short\0vector\0Vector2f\0Vector3f\0Vector4f\0\
m_ScriptingClassIdentifier\0Gradient\0Type*\0int2_storage\0int3_storage\0BoundsInt\0\
m_CorrespondingSourceObject\0m_PrefabInstance\0m_PrefabAsset\0FileSize\0Hash128\0\
RenderingLayerMask\0fixed_array\0EntityId\0LoadableObjectId\0LoadableSceneId\0";

/// The string starting exactly at `offset`, if `offset` is the start of an entry.
pub(crate) fn at(offset: u32) -> Option<&'static str> {
    let offset = offset as usize;
    if offset >= BUFFER.len() || (offset > 0 && BUFFER.as_bytes()[offset - 1] != 0) {
        return None;
    }
    BUFFER[offset..].split('\0').next()
}

#[cfg(test)]
pub(crate) fn offset_of(s: &str) -> u32 {
    let mut at = 0;
    for entry in BUFFER.split('\0') {
        if entry == s {
            return at;
        }
        at += entry.len() as u32 + 1;
    }
    panic!("{s} is not a common string")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_match_unitypy() {
        // Spot values from UnityPy's get_common_strings().
        assert_eq!(at(0), Some("AABB"));
        assert_eq!(at(49), Some("Array"));
        assert_eq!(at(1169), Some("RenderingLayerMask"));
        assert_eq!(at(1200), Some("EntityId"));
        assert_eq!(at(1226), Some("LoadableSceneId"));
        assert_eq!(at(1), None);
        assert_eq!(at(offset_of("long long")), Some("long long"));
    }
}

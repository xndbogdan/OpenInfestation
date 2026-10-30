use super::*;

const EYE: CameraParameterBlock = CameraParameterBlock {
    word_18: 0,
    table_1c: 0x0054_6960,
    table_20: 0x0054_6970,
};

fn initialized(position: [u16; 3], velocity: [u16; 3]) -> CameraParameterState {
    CameraParameterState {
        current_position: Some(position),
        current_velocity: Some(velocity),
        desired_position: Some([0; 3]),
        desired_velocity: Some([0; 3]),
    }
}

fn request(microseconds: u32) -> CameraSlewRequest {
    CameraSlewRequest {
        microseconds,
        snap: 0,
        terrain_bypass: 1,
    }
}

#[test]
fn snap_initializes_unknown_current_words_without_reading_a_table_or_terrain() {
    let mut state = CameraParameterState::UNWRITTEN;
    state.set_target([0xffff, 123, 456], [0x8000, 0xffff, 7]);
    assert!(state
        .advance(
            CameraParameterBlock { word_18: 8, ..EYE },
            CameraSlewRequest {
                snap: 1,
                ..request(0)
            },
            |_| panic!("snap queried terrain")
        )
        .unwrap());
    assert_eq!(state.current_position, state.desired_position);
    assert_eq!(state.current_velocity, state.desired_velocity);
    let mut unwritten = CameraParameterState::UNWRITTEN;
    assert_eq!(
        unwritten.advance(EYE, request(0), |_| Ok(0)),
        Err(CameraSlewError::UnwrittenTarget)
    );
}

#[test]
fn far_snap_is_strict_and_differences_wrap_before_signed_comparison() {
    let mut state = initialized([0; 3], [0; 3]);
    state.set_target([3000, 0, 0], [7, 8, 9]);
    assert!(!state.advance(EYE, request(0), |_| panic!()).unwrap());
    state.set_target([3001, 0, 0], [7, 8, 9]);
    assert!(state.advance(EYE, request(0), |_| panic!()).unwrap());
    assert_eq!(state.current_velocity, Some([7, 8, 9]));

    let mut wrap = initialized([65530, 0, 0], [0; 3]);
    assert!(!wrap.advance(EYE, request(32_000), |_| panic!()).unwrap());
    assert_eq!(wrap.current_position, Some([65530, 0, 0]));
}

#[test]
fn slew_uses_sqrt_of_distance_times_the_squared_coefficient() {
    let mut state = initialized([0; 3], [0; 3]);
    state.set_target([100, 0, 0], [0; 3]);
    assert!(!state.advance(EYE, request(32_000), |_| panic!()).unwrap());
    // Difference -100 narrows to -68 after the 32-unit deadzone. Speed is
    // floor(sqrt(68*40^2))=329; acceleration floor(5000*1000/32768)=152.
    assert_eq!(state.current_velocity, Some([152, 0, 0]));
    assert_eq!(state.current_position, Some([4, 0, 0]));
}

#[test]
fn terrain_mode_ignores_snap_and_moves_negative_velocity_toward_zero() {
    let mut state = initialized([1000, 1000, 1000], [(-100_i16) as u16, 0, 0]);
    state.set_target([1000, 1000, 1000], [(-100_i16) as u16, 0, 0]);
    let mut queries = Vec::new();
    assert!(!state
        .advance(
            EYE,
            CameraSlewRequest {
                microseconds: 32_000,
                snap: 1,
                terrain_bypass: 0
            },
            |xz| {
                queries.push(xz);
                Ok(600)
            }
        )
        .unwrap());
    assert_eq!(state.current_position, Some([997, 1056, 1000]));
    assert_eq!(queries, [[997, 1000]]);
}

#[test]
fn signed_distance_overflow_reports_the_native_division_fault() {
    let mut state = initialized([0x8000; 3], [0; 3]);
    assert_eq!(
        state.advance(
            EYE,
            CameraSlewRequest {
                terrain_bypass: 0,
                ..request(1)
            },
            |_| Ok(0)
        ),
        Err(CameraSlewError::DivisionFault)
    );
}

#[test]
fn parameter_mode_clamps_unsigned_values_and_preserves_table_identities() {
    let mut block = EYE;
    block.set_mode(u32::MAX);
    assert_eq!(block.word_18, 1);
    assert_eq!(
        (block.table_1c, block.table_20),
        (EYE.table_1c, EYE.table_20)
    );
    block.set_mode(0);
    assert_eq!(block.word_18, 0);
}

#[test]
fn raw_terrain_failure_keeps_velocity_and_position_written_before_the_query() {
    let mut state = initialized([1000, 1000, 1000], [(-100_i16) as u16, 0, 0]);
    state.set_target([1000, 1000, 1000], [(-100_i16) as u16, 0, 0]);
    let error = CameraSlewError::RawTerrain("unavailable".into());
    assert_eq!(
        state.advance(
            EYE,
            CameraSlewRequest {
                terrain_bypass: 0,
                ..request(32_000)
            },
            |_| Err(error.clone())
        ),
        Err(error)
    );
    assert_eq!(state.current_position, Some([997, 1000, 1000]));
    assert_eq!(state.current_velocity, Some([(-100_i16) as u16, 0, 0]));
}

/// The CRT table's entries 0052E00C..0052E028 jump to the initializers, each
/// of which pushes its deadzone, coefficient, far threshold and acceleration
/// (`6A ib` or `68 id`), loads its table into ECX and calls 0040EFC0. Each
/// table's constants are the port's.
#[test]
fn smoothing_tables_are_what_the_crt_initializers_build() {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let code = |address: u32, length: usize| &exe[(address - 0x0040_0000) as usize..][..length];
    let word = |address: u32| u32::from_le_bytes(code(address, 4).try_into().unwrap());
    let relative = |instruction: u32| (instruction + 5).wrapping_add(word(instruction + 1));
    let initializers = [
        (0x0040_eff0, 0x0040_f000),
        (0x0040_f020, 0x0040_f030),
        (0x0040_f050, 0x0040_f060),
        (0x0040_f080, 0x0040_f090),
        (0x0040_f0b0, 0x0040_f0c0),
        (0x0040_f0e0, 0x0040_f0f0),
        (0x0040_f110, 0x0040_f120),
        (0x0040_f140, 0x0040_f150),
    ];
    let mut tables = Vec::new();
    for (index, (jump, initializer)) in initializers.into_iter().enumerate() {
        assert_eq!(word(0x0052_e00c + 4 * index as u32), jump);
        assert_eq!(code(jump, 1), [0xe9]);
        assert_eq!(relative(jump), initializer);
        let mut at = initializer;
        let mut pushed = Vec::new();
        while pushed.len() < 4 {
            match code(at, 1)[0] {
                0x6a => {
                    pushed.push(i32::from(code(at + 1, 1)[0] as i8));
                    at += 2;
                }
                0x68 => {
                    pushed.push(word(at + 1) as i32);
                    at += 5;
                }
                other => panic!("{initializer:#x}: {other:#x}"),
            }
        }
        assert_eq!(code(at, 1), [0xb9]);
        let table = word(at + 1);
        assert_eq!(code(at + 5, 1), [0xe8]);
        assert_eq!(relative(at + 5), 0x0040_efc0);
        let block = CameraParameterBlock {
            word_18: 0,
            table_1c: table,
            table_20: table,
        };
        let expected = smoothing_table(block).unwrap();
        let [deadzone, coefficient, far, acceleration] = pushed[..] else {
            unreachable!()
        };
        assert_eq!(
            (
                expected.maximum_acceleration,
                expected.far_threshold,
                expected.speed_squared,
                expected.deadzone
            ),
            (acceleration, far, coefficient * coefficient, deadzone),
            "{initializer:#x}"
        );
        tables.push(table);
    }
    tables.sort_unstable();
    tables.dedup();
    assert_eq!(tables.len(), 8);
}

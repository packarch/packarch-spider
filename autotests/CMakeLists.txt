# Plain CMake/CTest replacement for ecm_add_test().
function(kpat_add_test name)
    cmake_parse_arguments(ARG "" "" "SOURCES;LINK_LIBRARIES" ${ARGN})
    add_executable(${name} ${ARG_SOURCES})
    target_link_libraries(${name} PRIVATE Qt6::Test ${ARG_LINK_LIBRARIES})
    target_include_directories(${name} PRIVATE ${CMAKE_SOURCE_DIR}/src ${CMAKE_BINARY_DIR}/src)
    add_test(NAME kpat-${name} COMMAND ${name} WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})
    set_tests_properties(kpat-${name} PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endfunction()

kpat_add_test(ShuffleTest SOURCES shuffle_test.cpp)

# These run the game binary (../bin/packarch-spider relative to the working directory).
kpat_add_test(FcSolveTest SOURCES freecell_solver.cpp)
kpat_add_test(SolveByVariantName SOURCES solve_by_name.cpp)
add_dependencies(FcSolveTest packarch-spider)
add_dependencies(SolveByVariantName packarch-spider)

if(WITH_BH_SOLVER)
    kpat_add_test(GolfSolveTest SOURCES golf_solver_wins.cpp)
    add_dependencies(GolfSolveTest packarch-spider)

    kpat_add_test(SolverFormatTest
        SOURCES solver_format.cpp
        LINK_LIBRARIES kpatcore)
endif()

# SMLib + SM_API (tangency-labs/smlib capi-tess @ 7eba8aa: NVIDIA usd-brep c9c5979 with
# memory-safety fixes, Apache-2.0; see fetch.sh) and the controller-cad facade.
#
# Settings follow solidcore's harness/cpp/CMakeLists.txt (which mirrors upstream premake):
#   defines NDEBUG NOMINMAX SM_GRAPHICS_CALLBACKS=1, flags -fexceptions -frtti -w, no TBB
#   (upstream compiles TBB code only under SM_USE_TBB*, which nothing defines).
#
# Used by
#   * crates/kernel-smlib/build.rs (native, via the `cmake` crate; targets SMLib, SM_API)
#   * smlib/build-wasm.sh (Emscripten; additionally builds the facade and links smlib.js)
#
# The upstream sources are not part of this repository; see smlib/fetch.sh.

cmake_minimum_required(VERSION 3.20)
project(controller_cad_smlib CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

set(SMLIB_SRC "" CACHE PATH "Path to the SMLib checkout (tangency-labs/smlib)")
if(NOT SMLIB_SRC)
  get_filename_component(SMLIB_SRC "${CMAKE_CURRENT_SOURCE_DIR}/../third_party/smlib" ABSOLUTE)
endif()
set(UP "${SMLIB_SRC}")
if(NOT EXISTS "${UP}/source/SMLib/inc/SmSmlibAll.h")
  message(FATAL_ERROR "SMLib sources not found at ${UP} (run smlib/fetch.sh or set SMLIB_SRC)")
endif()

option(SC_BUILD_FACADE "Build the controller-cad facade library" ON)
option(SC_WASM "Emscripten build: link smlib.js/smlib.wasm" OFF)

add_compile_definitions(NDEBUG NOMINMAX SM_GRAPHICS_CALLBACKS=1)
add_compile_options(-fexceptions -frtti -w)
if(EMSCRIPTEN)
  # Native Wasm exception handling (try/catch in the facade must work).
  add_compile_options(-fwasm-exceptions)
else()
  add_compile_definitions(_GLIBCXX_USE_CXX11_ABI=1)
endif()

# ---------------------------------------------------------------- SMLib
file(GLOB SMLIB_SOURCES CONFIGURE_DEPENDS "${UP}/source/SMLib/src/*.cpp")
add_library(SMLib STATIC ${SMLIB_SOURCES})
target_include_directories(SMLib PUBLIC "${UP}/source/SMLib/inc")

# ---------------------------------------------------------------- SM_API
file(GLOB SMAPI_SOURCES CONFIGURE_DEPENDS "${UP}/source/SM_API/src/*.cpp")
add_library(SM_API STATIC ${SMAPI_SOURCES})
target_include_directories(SM_API BEFORE PUBLIC "${UP}/source/SM_API/inc")
target_compile_definitions(SM_API PRIVATE SM_API_EXPORTS)
target_link_libraries(SM_API PUBLIC SMLib)

# ---------------------------------------------------------------- facade
if(SC_BUILD_FACADE)
  file(GLOB FACADE_SOURCES CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/facade/*.cpp")
  add_library(sc_facade STATIC ${FACADE_SOURCES})
  target_include_directories(sc_facade PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/facade")
  target_link_libraries(sc_facade PUBLIC SM_API SMLib)
endif()

if(SC_WASM)
  if(NOT EMSCRIPTEN)
    message(FATAL_ERROR "SC_WASM requires the Emscripten toolchain (emcmake)")
  endif()
  # A tiny translation unit so the executable target has a source; all exports
  # come from the facade archive (kept alive via EXPORTED_FUNCTIONS).
  file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/sc_wasm_main.cpp" "int main() { return 0; }\n")
  add_executable(smlib "${CMAKE_CURRENT_BINARY_DIR}/sc_wasm_main.cpp")
  target_link_libraries(smlib PRIVATE sc_facade)
  file(READ "${CMAKE_CURRENT_SOURCE_DIR}/facade/exports.txt" _exports)
  string(REGEX REPLACE "[\r\n]+" ";" _exports "${_exports}")
  set(_exp "")
  foreach(_e IN LISTS _exports)
    string(STRIP "${_e}" _e)
    if(_e AND NOT _e MATCHES "^#")
      list(APPEND _exp "'_${_e}'")
    endif()
  endforeach()
  list(APPEND _exp "'_malloc'" "'_free'")
  list(JOIN _exp "," _exp_joined)
  set_target_properties(smlib PROPERTIES SUFFIX ".js")
  target_link_options(smlib PRIVATE
    -fwasm-exceptions
    "SHELL:-s MODULARIZE=1"
    "SHELL:-s EXPORT_ES6=1"
    "SHELL:-s EXPORT_NAME=createSmlib"
    "SHELL:-s ALLOW_MEMORY_GROWTH=1"
    "SHELL:-s INITIAL_MEMORY=33554432"
    "SHELL:-s MAXIMUM_MEMORY=4294967296"
    "SHELL:-s STACK_SIZE=8388608"
    "SHELL:-s ENVIRONMENT=worker,web,node"
    "SHELL:-s FILESYSTEM=0"
    "SHELL:-s INVOKE_RUN=0"
    "SHELL:-s EXIT_RUNTIME=0"
    "SHELL:-s EXPORTED_FUNCTIONS=[${_exp_joined}]"
    "SHELL:-s EXPORTED_RUNTIME_METHODS=['HEAPU8','HEAP32','HEAPU32','HEAPF32','HEAPF64','UTF8ToString']"
  )
endif()

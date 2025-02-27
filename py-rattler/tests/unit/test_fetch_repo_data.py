# type: ignore
import os.path
import subprocess

import pytest
from rattler import Channel, ChannelConfig, fetch_repo_data, SparseRepoData, PackageName
from rattler.platform import Platform
from rattler.repo_data.record import RepoDataRecord


@pytest.fixture(scope="session")
def serve_repo_data() -> None:
    port, repo_name = 8912, "test-repo"

    test_data_dir = os.path.join(
        os.path.dirname(__file__), "../../../test-data/test-server"
    )

    with subprocess.Popen(
        [
            "python",
            os.path.join(test_data_dir, "reposerver.py"),
            "-d",
            os.path.join(test_data_dir, "repo"),
            "-n",
            repo_name,
            "-p",
            str(port),
        ]
    ) as proc:
        yield port, repo_name
        proc.terminate()


@pytest.mark.asyncio
async def test_fetch_repo_data(
    tmp_path,
    serve_repo_data,
):
    port, repo = serve_repo_data
    cache_dir = tmp_path / "test_repo_data_download"
    chan = Channel(repo, ChannelConfig(f"http://localhost:{port}/"))
    plat = Platform("noarch")

    result = await fetch_repo_data(
        channels=[chan],
        platforms=[plat],
        cache_path=cache_dir,
        callback=None,
    )
    assert isinstance(result, list)

    repodata = result[0]
    assert isinstance(repodata, SparseRepoData)

    package = PackageName(repodata.package_names()[0])
    repodata_record = repodata.load_records(package)[0]
    assert isinstance(repodata_record, RepoDataRecord)

    assert repodata_record.channel == f"http://localhost:{port}/{repo}/"
    assert repodata_record.file_name == "test-package-0.1-0.tar.bz2"
    assert str(repodata_record.package_record) == "test-package=0.1=0"
    assert (
        repodata_record.url
        == f"http://localhost:{port}/test-repo/noarch/test-package-0.1-0.tar.bz2"
    )

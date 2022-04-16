import asyncio
import sys
from compileall import compile_file
from concurrent.futures import ProcessPoolExecutor


def compile_one(file):
    result = compile_file(file, quiet=1)
    return file, result


async def main():
    # max_workers = int(os.environ.get("MAMBA_EXTRACT_THREADS", "0"))
    # if max_workers <= 0:
    #     max_workers = None
    #

    success = True
    results = []
    with sys.stdin as stdin:
        # Get a reader for stdin
        loop = asyncio.get_event_loop()
        reader = asyncio.StreamReader()
        protocol = asyncio.StreamReaderProtocol(reader)
        await loop.connect_read_pipe(lambda: protocol, stdin)

        # With a process pool to run our compilations
        with ProcessPoolExecutor() as executor:
            read_line_future = loop.create_task(reader.readuntil(b'\n'))
            remaining_futures = {read_line_future}
            while True:
                # Wait for the first future to finish
                finished_futures, remaining_futures = await asyncio.wait(remaining_futures, return_when=asyncio.FIRST_COMPLETED)
                for finished_future in finished_futures:
                    # If the read line future is done, queue a new item
                    if read_line_future is finished_future:
                        if read_line_future.exception() is not None:
                            return success

                        # Decode the input as utf-8
                        input = read_line_future.result().decode('utf-8').strip()

                        # Submit the execution to the process pool
                        remaining_futures.add(loop.run_in_executor(executor, compile_one, input))

                        # Queue another read-line task
                        read_line_future = loop.create_task(reader.readuntil(b'\n'))
                        remaining_futures.add(read_line_future)

                    # Otherwise, a compilation finished
                    else:
                        finished_file, result = finished_future.result()
                        if not result:
                            success = False
                        print(finished_file)


if __name__ == "__main__":
    success = asyncio.run(main())
    sys.exit(int(not success))

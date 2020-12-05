package main

import (
	"errors"
	"fmt"
	"log"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"time"
)

var StandardClient = &http.Client{
	Timeout: time.Second * 30,
}

var noFollowClient = &http.Client{
	CheckRedirect: func(req *http.Request, via []*http.Request) error {
		return http.ErrUseLastResponse
	},
	Timeout: time.Second * 30,
}

var visited = make(map[string]bool)
var boundaryPrefix string
var boundaryHost string // same domain with different ports is acceptable here.

func get(url *url.URL, client *http.Client) (*http.Response, error) {
	urlString := url.String()
	resp, err := client.Get(urlString)
	if err != nil {
		return nil, err
	}
	return resp, nil
}

func noFollowGet(url *url.URL, maxTries int) (*http.Response, error) {
	if maxTries == 0 {
		maxTries = 1
	}
	tries := 0

	for {
		if tries > maxTries {
			break
		}
		if tries >= 1 {
			if visited[url.String()] {
				log.Printf("Redirected URL %s has been visited before.", url.String())
				break
			} else {
				visited[url.String()] = true
			}
		}
		tries++
		resp, err := get(url, noFollowClient)
		if err != nil {
			log.Println(err)
			continue
		}
		if IsRedirect(resp) {
			location, err := getRedirectLocation(resp)
			if err != nil {
				log.Fatal(err)
			}
			fmt.Printf("%d: %s -> %s\n", resp.StatusCode, url.String(), location)
			err = isURLOutOfBoundary(location)
			if err != nil {
				return nil, err
			}
			url = location
		} else {
			return resp, nil
		}
	}
	// last try with StandardClient
	resp, err := get(url, StandardClient)
	if err != nil {
		return resp, err
	}
	if visited[resp.Request.URL.String()] {
		return nil, errors.New("visited URL, ignored")
	}
	err = isURLOutOfBoundary(resp.Request.URL)
	return resp, err
}

func download(url *url.URL, baseFolder string) {
	if !visited[url.String()] {
		visited[url.String()] = true
		resp, err := noFollowGet(url, 1)
		if err != nil {
			log.Println(err)
			return
		}
		defer resp.Body.Close()
		url = resp.Request.URL // update URL, as 30x may modify it.
		fmt.Printf("%s: %d\n", url.String(), resp.StatusCode)
		statusOK := resp.StatusCode >= 200 && resp.StatusCode < 300
		if !statusOK {
			log.Printf("URL %s got %d\n", url.String(), resp.StatusCode)
		} else {
			if IsHTML(resp.Header) {
				folderPath := getFileRelPath(url)
				folderPath = filepath.Join(baseFolder, folderPath)
				err = os.MkdirAll(folderPath, 0755)
				if err != nil {
					log.Printf("Create %s failed: %v\n", folderPath, err)
					return
				}
				hrefs := getHrefsFromHTML(resp.Body)
				for _, href := range hrefs {
					newURL, err := urlBuilder(url, href)
					if err != nil {
						log.Printf("Failed when building URL %s with %s: %v\n", url.String(), href, err)
					} else {
						download(newURL, baseFolder)
					}
				}
			} else {
				downloadPath := getFileRelPath(url)
				downloadPath = filepath.Join(baseFolder, downloadPath)
				if _, err := os.Stat(downloadPath); os.IsNotExist(err) {
					fmt.Printf("Downloading %s -> %s\n", url.String(), downloadPath)
					fmt.Println("Dry run (not actually downloading)")

					// TODO: create a tmp file and then rename if ok
					out, err := os.Create(downloadPath)
					if err != nil {
						log.Println(err)
						return
					}
					defer out.Close()

					/* _, err = io.Copy(out, resp.Body)
					if err != nil {
						log.Fatal(err)
					} */
				} else {
					fmt.Printf("%s exists.\n", downloadPath)
				}
				return
			}
		}
	}
}

func main() {
	baseString := "https://download.docker.com/"
	targetString := "https://download.docker.com/linux/centos/8.4/"
	target, err := url.Parse(targetString)
	if err != nil {
		log.Fatal(err)
	}
	base, err := url.Parse(baseString)
	if err != nil {
		log.Fatal(err)
	}

	validateEnterpoint(target)
	validateEnterpoint(base)

	boundaryPrefix = base.Path
	boundaryHost = base.Hostname()

	fmt.Printf("%v %v", target.Path, boundaryPrefix)
	if err != nil {
		log.Fatal(err)
	}
	download(target, "/tmp/test")
}
